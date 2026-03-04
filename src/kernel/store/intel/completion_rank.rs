use crate::kernel::language::LanguageId;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

const MAX_ENTRIES_PER_LANGUAGE: usize = 512;

#[cfg(test)]
use std::cell::RefCell;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct CompletionRankerPerfCounters {
    pub(super) score_calls: usize,
}

#[cfg(test)]
thread_local! {
    static PERF_COUNTERS: RefCell<CompletionRankerPerfCounters> =
        RefCell::new(CompletionRankerPerfCounters::default());
}

#[cfg(test)]
fn with_perf_counters(mut f: impl FnMut(&mut CompletionRankerPerfCounters)) {
    PERF_COUNTERS.with(|counters| {
        let mut counters = counters.borrow_mut();
        f(&mut counters);
    });
}

#[cfg(test)]
fn add_score_calls(value: usize) {
    with_perf_counters(|counters| {
        counters.score_calls = counters.score_calls.saturating_add(value);
    });
}

#[derive(Debug, Clone)]
pub struct CompletionRanker {
    index: FxHashMap<LanguageId, LanguageRankState>,
    dirty: bool,
    // Monotonic stamp, used to order items by most-recently-confirmed completion.
    clock: f64,
}

#[derive(Debug, Clone, Default)]
struct LanguageRankState {
    // kind -> (label -> last_used_stamp)
    last_used_by_kind: FxHashMap<Option<u32>, FxHashMap<String, f64>>,
    entry_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct CompletionRankerData {
    #[serde(default)]
    languages: Vec<LanguageRankerEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct LanguageRankerEntry {
    language: LanguageId,
    frequency: LanguageFrequency,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct LanguageFrequency {
    items: Vec<FrequencyEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct FrequencyEntry {
    label: String,
    kind: Option<u32>,
    score: f64,
}

impl LanguageRankState {
    fn from_frequency(frequency: LanguageFrequency) -> Self {
        let mut state = Self::default();
        for item in frequency.items {
            if item.score <= 0.0 {
                continue;
            }
            let bucket = state.last_used_by_kind.entry(item.kind).or_default();
            if bucket.insert(item.label, item.score).is_none() {
                state.entry_count = state.entry_count.saturating_add(1);
            }
        }
        state.enforce_capacity();
        state
    }

    fn score(&self, label: &str, kind: Option<u32>) -> f64 {
        let Some(bucket) = self.last_used_by_kind.get(&kind) else {
            return 0.0;
        };
        bucket.get(label).copied().unwrap_or(0.0)
    }

    fn record(&mut self, label: &str, kind: Option<u32>, stamp: f64) {
        let bucket = self.last_used_by_kind.entry(kind).or_default();
        if bucket.insert(label.to_string(), stamp).is_none() {
            self.entry_count = self.entry_count.saturating_add(1);
        }
        self.enforce_capacity();
    }

    fn max_stamp(&self) -> f64 {
        let mut max = 0.0f64;
        for bucket in self.last_used_by_kind.values() {
            for &stamp in bucket.values() {
                if stamp.is_finite() && stamp > max {
                    max = stamp;
                }
            }
        }
        max
    }

    fn enforce_capacity(&mut self) {
        if self.entry_count <= MAX_ENTRIES_PER_LANGUAGE {
            return;
        }

        let mut ranked = Vec::with_capacity(self.entry_count);
        for (&kind, bucket) in &self.last_used_by_kind {
            for (label, &stamp) in bucket {
                ranked.push((stamp, kind, label.clone()));
            }
        }

        ranked.sort_by(|a, b| {
            b.0.partial_cmp(&a.0)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.1.cmp(&b.1))
                .then_with(|| a.2.cmp(&b.2))
        });
        ranked.truncate(MAX_ENTRIES_PER_LANGUAGE);

        self.last_used_by_kind.clear();
        for (stamp, kind, label) in ranked {
            self.last_used_by_kind
                .entry(kind)
                .or_default()
                .insert(label, stamp);
        }

        self.entry_count = self
            .last_used_by_kind
            .values()
            .map(FxHashMap::len)
            .sum::<usize>();
    }

    fn snapshot_items(&self) -> Vec<FrequencyEntry> {
        let mut items = Vec::with_capacity(self.entry_count);
        for (&kind, bucket) in &self.last_used_by_kind {
            for (label, &stamp) in bucket {
                items.push(FrequencyEntry {
                    label: label.clone(),
                    kind,
                    score: stamp,
                });
            }
        }

        items.sort_by(|a, b| {
            a.kind
                .cmp(&b.kind)
                .then_with(|| a.label.cmp(&b.label))
                .then_with(|| {
                    b.score
                        .partial_cmp(&a.score)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
        });
        items
    }
}

impl CompletionRanker {
    fn from_data(data: CompletionRankerData) -> Self {
        let mut index = FxHashMap::default();
        for entry in data.languages {
            let state = LanguageRankState::from_frequency(entry.frequency);
            if state.entry_count > 0 {
                index.insert(entry.language, state);
            }
        }

        let mut clock = 0.0f64;
        for state in index.values() {
            clock = clock.max(state.max_stamp());
        }

        Self {
            index,
            dirty: false,
            clock: clock.ceil(),
        }
    }

    fn snapshot_data(&self) -> CompletionRankerData {
        let mut languages = Vec::with_capacity(self.index.len());
        for (&language, state) in &self.index {
            let items = state.snapshot_items();

            languages.push(LanguageRankerEntry {
                language,
                frequency: LanguageFrequency { items },
            });
        }

        languages.sort_by(|a, b| a.language.language_id().cmp(b.language.language_id()));

        CompletionRankerData { languages }
    }

    pub fn from_deserialized(self) -> Self {
        self
    }

    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    #[cfg(test)]
    pub(super) fn reset_perf_counters() {
        PERF_COUNTERS.with(|counters| {
            *counters.borrow_mut() = CompletionRankerPerfCounters::default();
        });
    }

    #[cfg(test)]
    pub(super) fn perf_counters() -> CompletionRankerPerfCounters {
        PERF_COUNTERS.with(|counters| *counters.borrow())
    }

    pub fn record(&mut self, language: Option<LanguageId>, label: &str, kind: Option<u32>) {
        let Some(lang) = language else {
            return;
        };

        self.clock += 1.0;
        let stamp = self.clock;
        let state = self.index.entry(lang).or_default();
        state.record(label, kind, stamp);
        self.dirty = true;
    }

    pub fn score(&self, language: Option<LanguageId>, label: &str, kind: Option<u32>) -> f64 {
        #[cfg(test)]
        {
            add_score_calls(1);
        }

        let Some(lang) = language else {
            return 0.0;
        };
        let Some(state) = self.index.get(&lang) else {
            return 0.0;
        };
        state.score(label, kind)
    }
}

impl Default for CompletionRanker {
    fn default() -> Self {
        Self {
            index: FxHashMap::default(),
            dirty: false,
            clock: 0.0,
        }
    }
}

impl Serialize for CompletionRanker {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.snapshot_data().serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for CompletionRanker {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let data = CompletionRankerData::deserialize(deserializer)?;
        Ok(Self::from_data(data))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_and_score() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        assert!(ranker.score(Some(LanguageId::C), "printf", Some(3)) > 0.0);
        assert_eq!(ranker.score(Some(LanguageId::C), "puts", Some(3)), 0.0);
    }

    #[test]
    fn record_sets_item_as_most_recent() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        ranker.record(Some(LanguageId::C), "puts", Some(3));
        assert!(
            ranker.score(Some(LanguageId::C), "puts", Some(3))
                > ranker.score(Some(LanguageId::C), "printf", Some(3))
        );
    }

    #[test]
    fn record_again_moves_item_to_front() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        ranker.record(Some(LanguageId::C), "puts", Some(3));
        ranker.record(Some(LanguageId::C), "printf", Some(3));

        assert!(
            ranker.score(Some(LanguageId::C), "printf", Some(3))
                > ranker.score(Some(LanguageId::C), "puts", Some(3))
        );
    }

    #[test]
    fn languages_are_independent() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::Rust), "unwrap", Some(2));
        assert!(ranker.score(Some(LanguageId::Rust), "unwrap", Some(2)) > 0.0);
        assert_eq!(
            ranker.score(Some(LanguageId::Python), "unwrap", Some(2)),
            0.0
        );
    }

    #[test]
    fn kind_distinguishes_items() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::Python), "print", Some(3));
        assert!(ranker.score(Some(LanguageId::Python), "print", Some(3)) > 0.0);
        assert_eq!(
            ranker.score(Some(LanguageId::Python), "print", Some(6)),
            0.0
        );
    }

    #[test]
    fn no_language_is_noop() {
        let mut ranker = CompletionRanker::default();
        ranker.record(None, "foo", Some(1));
        assert_eq!(ranker.score(None, "foo", Some(1)), 0.0);
    }

    #[test]
    fn capacity_limit_enforced() {
        let mut ranker = CompletionRanker::default();
        for i in 0..600 {
            ranker.record(Some(LanguageId::C), &format!("item_{i}"), Some(1));
        }
        let state = ranker.index.get(&LanguageId::C).unwrap();
        assert!(state.entry_count <= MAX_ENTRIES_PER_LANGUAGE);
        assert_eq!(ranker.score(Some(LanguageId::C), "item_0", Some(1)), 0.0);
        assert!(ranker.score(Some(LanguageId::C), "item_599", Some(1)) > 0.0);
    }

    #[test]
    fn serde_roundtrip() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        ranker.record(Some(LanguageId::Rust), "unwrap", Some(2));

        let json = serde_json::to_string(&ranker).unwrap();
        let loaded: CompletionRanker = serde_json::from_str::<CompletionRanker>(&json)
            .unwrap()
            .from_deserialized();

        assert!(
            (loaded.score(Some(LanguageId::C), "printf", Some(3))
                - ranker.score(Some(LanguageId::C), "printf", Some(3)))
            .abs()
                < f64::EPSILON
        );
        assert!(
            (loaded.score(Some(LanguageId::Rust), "unwrap", Some(2))
                - ranker.score(Some(LanguageId::Rust), "unwrap", Some(2)))
            .abs()
                < f64::EPSILON
        );
    }

    #[test]
    fn dirty_flag_can_be_cleared_after_save() {
        let mut ranker = CompletionRanker::default();
        assert!(!ranker.is_dirty());

        ranker.record(Some(LanguageId::Rust), "println", Some(3));
        assert!(ranker.is_dirty());

        let _json = serde_json::to_string(&ranker).unwrap();
        assert!(ranker.is_dirty());

        ranker.clear_dirty();
        assert!(!ranker.is_dirty());
    }
}

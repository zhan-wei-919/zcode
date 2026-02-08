use crate::kernel::language::LanguageId;
use rustc_hash::FxHashMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[cfg(test)]
use std::cell::RefCell;

const DECAY_FACTOR: f64 = 0.95;
const MAX_ENTRIES_PER_LANGUAGE: usize = 512;
const MIN_SCORE: f64 = 0.001;
const CLEANUP_INTERVAL: u32 = 32;
const MIN_SCALE: f64 = 1e-6;

#[cfg(test)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(super) struct CompletionRankerPerfCounters {
    pub(super) score_calls: usize,
    pub(super) sync_calls: usize,
    pub(super) decay_visits: usize,
    pub(super) sync_item_visits: usize,
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
fn add_decay_visits(value: usize) {
    with_perf_counters(|counters| {
        counters.decay_visits = counters.decay_visits.saturating_add(value);
    });
}

#[cfg(test)]
fn add_sync_calls(value: usize) {
    with_perf_counters(|counters| {
        counters.sync_calls = counters.sync_calls.saturating_add(value);
    });
}

#[cfg(test)]
fn add_sync_item_visits(value: usize) {
    with_perf_counters(|counters| {
        counters.sync_item_visits = counters.sync_item_visits.saturating_add(value);
    });
}

#[cfg(test)]
fn add_score_calls(value: usize) {
    with_perf_counters(|counters| {
        counters.score_calls = counters.score_calls.saturating_add(value);
    });
}

#[derive(Debug, Clone, Default)]
pub struct CompletionRanker {
    index: FxHashMap<LanguageId, LanguageRankState>,
    dirty: bool,
}

#[derive(Debug, Clone)]
struct LanguageRankState {
    scale: f64,
    updates_since_cleanup: u32,
    // kind -> (label -> raw score)
    // actual score = raw score * scale
    scores_by_kind: FxHashMap<Option<u32>, FxHashMap<String, f64>>,
    entry_count: usize,
}

impl Default for LanguageRankState {
    fn default() -> Self {
        Self {
            scale: 1.0,
            updates_since_cleanup: 0,
            scores_by_kind: FxHashMap::default(),
            entry_count: 0,
        }
    }
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
            if item.score < MIN_SCORE {
                continue;
            }
            let bucket = state.scores_by_kind.entry(item.kind).or_default();
            if bucket.insert(item.label, item.score).is_none() {
                state.entry_count = state.entry_count.saturating_add(1);
            }
        }
        state
    }

    fn score(&self, label: &str, kind: Option<u32>) -> f64 {
        let Some(bucket) = self.scores_by_kind.get(&kind) else {
            return 0.0;
        };
        bucket.get(label).copied().unwrap_or(0.0) * self.scale
    }

    fn record(&mut self, label: &str, kind: Option<u32>) {
        self.scale *= DECAY_FACTOR;

        if self.scale < MIN_SCALE {
            self.normalize_scale();
        }

        let add = 1.0 / self.scale;
        let bucket = self.scores_by_kind.entry(kind).or_default();
        if let Some(score) = bucket.get_mut(label) {
            *score += add;
        } else {
            bucket.insert(label.to_string(), add);
            self.entry_count = self.entry_count.saturating_add(1);
        }

        self.updates_since_cleanup = self.updates_since_cleanup.saturating_add(1);
        if self.entry_count > MAX_ENTRIES_PER_LANGUAGE
            || self.updates_since_cleanup >= CLEANUP_INTERVAL
        {
            self.cleanup();
            self.updates_since_cleanup = 0;
        }
    }

    fn normalize_scale(&mut self) {
        if (self.scale - 1.0).abs() <= f64::EPSILON {
            return;
        }
        let scale = self.scale;
        for bucket in self.scores_by_kind.values_mut() {
            for score in bucket.values_mut() {
                #[cfg(test)]
                {
                    add_decay_visits(1);
                }
                *score *= scale;
            }
        }
        self.scale = 1.0;
    }

    fn cleanup(&mut self) {
        let threshold = MIN_SCORE / self.scale;

        let mut kept = 0usize;
        self.scores_by_kind.retain(|_, bucket| {
            bucket.retain(|_, score| {
                #[cfg(test)]
                {
                    add_decay_visits(1);
                }
                *score >= threshold
            });
            kept = kept.saturating_add(bucket.len());
            !bucket.is_empty()
        });
        self.entry_count = kept;

        if self.entry_count <= MAX_ENTRIES_PER_LANGUAGE {
            return;
        }

        let mut ranked = Vec::with_capacity(self.entry_count);
        for (&kind, bucket) in &self.scores_by_kind {
            for (label, &score) in bucket {
                ranked.push((kind, label.clone(), score));
            }
        }

        ranked.sort_by(|a, b| {
            b.2.partial_cmp(&a.2)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.0.cmp(&b.0))
                .then_with(|| a.1.cmp(&b.1))
        });
        ranked.truncate(MAX_ENTRIES_PER_LANGUAGE);

        self.scores_by_kind.clear();
        for (kind, label, score) in ranked {
            self.scores_by_kind
                .entry(kind)
                .or_default()
                .insert(label, score);
        }

        self.entry_count = self
            .scores_by_kind
            .values()
            .map(FxHashMap::len)
            .sum::<usize>();
    }

    fn snapshot_items(&self) -> Vec<FrequencyEntry> {
        let mut items = Vec::with_capacity(self.entry_count);
        for (&kind, bucket) in &self.scores_by_kind {
            for (label, &raw_score) in bucket {
                items.push(FrequencyEntry {
                    label: label.clone(),
                    kind,
                    score: raw_score * self.scale,
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

        Self {
            index,
            dirty: false,
        }
    }

    fn snapshot_data(&self) -> CompletionRankerData {
        #[cfg(test)]
        let mut synced_items = 0usize;

        let mut languages = Vec::with_capacity(self.index.len());
        for (&language, state) in &self.index {
            let items = state.snapshot_items();

            #[cfg(test)]
            {
                synced_items = synced_items.saturating_add(items.len());
            }

            languages.push(LanguageRankerEntry {
                language,
                frequency: LanguageFrequency { items },
            });
        }

        languages.sort_by(|a, b| a.language.language_id().cmp(b.language.language_id()));

        #[cfg(test)]
        {
            add_sync_calls(1);
            add_sync_item_visits(synced_items);
        }

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

        let state = self.index.entry(lang).or_default();
        state.record(label, kind);
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
    fn decay_reduces_old_scores() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        let s1 = ranker.score(Some(LanguageId::C), "printf", Some(3));

        ranker.record(Some(LanguageId::C), "puts", Some(3));
        let s2 = ranker.score(Some(LanguageId::C), "printf", Some(3));
        assert!(s2 < s1);
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
    }

    #[test]
    fn frequency_ordering() {
        let mut ranker = CompletionRanker::default();
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        ranker.record(Some(LanguageId::C), "printf", Some(3));
        ranker.record(Some(LanguageId::C), "puts", Some(3));

        let s_printf = ranker.score(Some(LanguageId::C), "printf", Some(3));
        let s_puts = ranker.score(Some(LanguageId::C), "puts", Some(3));
        assert!(s_printf > s_puts);
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

    #[test]
    fn experiment_record_avoids_full_sync_and_full_decay_walk() {
        let mut ranker = CompletionRanker::default();
        CompletionRanker::reset_perf_counters();

        let n = 100usize;
        for i in 0..n {
            ranker.record(Some(LanguageId::C), &format!("item_{i}"), Some(1));
        }

        let counters = CompletionRanker::perf_counters();
        eprintln!(
            "[experiment] record sync_calls={} decay_visits={} sync_item_visits={} n={}",
            counters.sync_calls, counters.decay_visits, counters.sync_item_visits, n
        );

        assert_eq!(counters.sync_calls, 0);
        assert_eq!(counters.sync_item_visits, 0);
        assert!(
            counters.decay_visits < n * 8,
            "decay_visits={} expected_less_than={}",
            counters.decay_visits,
            n * 8
        );

        let _json = serde_json::to_string(&ranker).unwrap();
        let after_save = CompletionRanker::perf_counters();
        assert!(after_save.sync_calls >= 1);
        assert!(after_save.sync_item_visits > 0);
    }
}

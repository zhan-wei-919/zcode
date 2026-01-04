use std::time::Duration;

#[cfg(feature = "perf")]
use rustc_hash::FxHashMap;
#[cfg(feature = "perf")]
use std::cell::RefCell;
#[cfg(feature = "perf")]
use std::time::Instant;

#[derive(Debug, Clone, Copy)]
pub struct PerfSample {
    pub label: &'static str,
    pub count: u64,
    pub total: Duration,
    pub max: Duration,
}

#[cfg(feature = "perf")]
#[derive(Debug, Default, Clone, Copy)]
struct Stats {
    count: u64,
    total: Duration,
    max: Duration,
}

#[cfg(feature = "perf")]
thread_local! {
    static METRICS: RefCell<FxHashMap<&'static str, Stats>> = RefCell::new(FxHashMap::default());
}

#[cfg(feature = "perf")]
pub struct Scope {
    label: &'static str,
    start: Instant,
}

#[cfg(not(feature = "perf"))]
pub struct Scope;

#[inline]
pub fn scope(label: &'static str) -> Scope {
    #[cfg(feature = "perf")]
    {
        Scope {
            label,
            start: Instant::now(),
        }
    }
    #[cfg(not(feature = "perf"))]
    {
        let _ = label;
        Scope
    }
}

#[cfg(feature = "perf")]
impl Drop for Scope {
    fn drop(&mut self) {
        let elapsed = self.start.elapsed();
        METRICS.with(|metrics| {
            let mut metrics = metrics.borrow_mut();
            let entry = metrics.entry(self.label).or_insert_with(Stats::default);
            entry.count += 1;
            entry.total += elapsed;
            entry.max = entry.max.max(elapsed);
        });
    }
}

pub fn snapshot() -> Vec<PerfSample> {
    #[cfg(feature = "perf")]
    {
        METRICS.with(|metrics| {
            metrics
                .borrow()
                .iter()
                .map(|(label, stats)| PerfSample {
                    label: *label,
                    count: stats.count,
                    total: stats.total,
                    max: stats.max,
                })
                .collect()
        })
    }

    #[cfg(not(feature = "perf"))]
    {
        Vec::new()
    }
}

pub fn reset() {
    #[cfg(feature = "perf")]
    {
        METRICS.with(|metrics| metrics.borrow_mut().clear());
    }
}

pub fn report_and_reset() -> String {
    let mut samples = snapshot();
    if samples.is_empty() {
        return String::new();
    }

    samples.sort_by(|a, b| b.total.cmp(&a.total));
    let mut out = String::new();
    for sample in samples {
        let avg = if sample.count > 0 {
            sample.total.as_secs_f64() * 1_000_000.0 / sample.count as f64
        } else {
            0.0
        };
        let total_ms = sample.total.as_secs_f64() * 1000.0;
        let max_us = sample.max.as_secs_f64() * 1_000_000.0;
        out.push_str(&format!(
            "{:<28} count={:<8} total_ms={:>10.3} avg_us={:>10.3} max_us={:>10.3}\n",
            sample.label,
            sample.count,
            total_ms,
            avg,
            max_us
        ));
    }

    reset();
    out
}

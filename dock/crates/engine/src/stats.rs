//! CPU and memory figures computed the way `docker stats` does, network and
//! block I/O totals, and a short per-container history for sparklines.

use crate::model::Stats;
use std::collections::{HashMap, VecDeque};

/// Raw CPU counters of one stats sample (current or previous).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CpuSample {
    pub total_usage: u64,
    pub system_usage: u64,
}

/// CPU percent between two samples; 100 % = one core fully used.
#[must_use]
pub fn cpu_percent(previous: CpuSample, current: CpuSample, online_cpus: u32) -> f64 {
    let cpu_delta = current.total_usage.saturating_sub(previous.total_usage);
    let system_delta = current.system_usage.saturating_sub(previous.system_usage);
    if cpu_delta == 0 || system_delta == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = cpu_delta as f64 / system_delta as f64;
    ratio * f64::from(online_cpus.max(1)) * 100.0
}

/// Memory in use excluding the reclaimable page cache: cgroup v2 reports
/// `inactive_file`, cgroup v1 `total_inactive_file` (same as the Docker CLI).
#[must_use]
pub fn memory_usage(usage: u64, stats: Option<&HashMap<String, u64>>) -> u64 {
    let cache = stats
        .and_then(|s| {
            s.get("inactive_file")
                .or_else(|| s.get("total_inactive_file"))
        })
        .copied()
        .unwrap_or(0);
    if cache < usage { usage - cache } else { usage }
}

/// Sums block I/O entries (`op`, bytes) into (read, write). cgroup v1 says
/// `Read`/`Write`, cgroup v2 `read`/`write`; other ops (`Sync`, `Total`…) are ignored.
#[must_use]
pub fn block_io<'a>(entries: impl IntoIterator<Item = (&'a str, u64)>) -> (u64, u64) {
    entries
        .into_iter()
        .fold((0, 0), |(read, write), (op, value)| {
            if op.eq_ignore_ascii_case("read") {
                (read.saturating_add(value), write)
            } else if op.eq_ignore_ascii_case("write") {
                (read, write.saturating_add(value))
            } else {
                (read, write)
            }
        })
}

/// Samples kept per container (at the 2 s poll: two minutes).
pub const HISTORY_LEN: usize = 60;

/// Bytes per second between the two most recent samples.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct IoRates {
    pub net_rx: f64,
    pub net_tx: f64,
    pub blk_read: f64,
    pub blk_write: f64,
}

/// Recent samples of one container, oldest first, with their times.
#[derive(Debug, Clone, Default)]
pub struct StatsHistory {
    samples: VecDeque<(f64, Stats)>,
}

impl StatsHistory {
    /// Adds a sample taken at `at` seconds (any monotonic clock).
    pub fn push(&mut self, at: f64, stats: Stats) {
        self.samples.push_back((at, stats));
        while self.samples.len() > HISTORY_LEN {
            self.samples.pop_front();
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.samples.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    #[must_use]
    pub fn latest(&self) -> Option<&Stats> {
        self.samples.back().map(|(_, s)| s)
    }

    #[must_use]
    pub fn cpu(&self) -> Vec<f64> {
        self.samples.iter().map(|(_, s)| s.cpu_percent).collect()
    }

    /// Memory in bytes (as f64 for plotting).
    #[must_use]
    pub fn memory(&self) -> Vec<f64> {
        #[allow(clippy::cast_precision_loss)]
        self.samples
            .iter()
            .map(|(_, s)| s.mem_usage as f64)
            .collect()
    }

    /// Rates between the last two samples. Counters that went backwards
    /// (container restarted) give 0 rather than a negative rate.
    #[must_use]
    pub fn rates(&self) -> Option<IoRates> {
        let n = self.samples.len();
        if n < 2 {
            return None;
        }
        let (t0, a) = self.samples[n - 2];
        let (t1, b) = self.samples[n - 1];
        let dt = t1 - t0;
        if dt <= 0.0 {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        let rate = |x: u64, y: u64| y.saturating_sub(x) as f64 / dt;
        Some(IoRates {
            net_rx: rate(a.net_rx, b.net_rx),
            net_tx: rate(a.net_tx, b.net_tx),
            blk_read: rate(a.blk_read, b.blk_read),
            blk_write: rate(a.blk_write, b.blk_write),
        })
    }
}

/// Heights in 0..=1 for a sparkline: values scaled to `max(values, floor)`,
/// so an idle container doesn't draw noise at full height.
#[must_use]
pub fn sparkline(values: &[f64], floor: f64) -> Vec<f32> {
    let max = values
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(floor, f64::max);
    if max <= 0.0 {
        return vec![0.0; values.len()];
    }
    #[allow(clippy::cast_possible_truncation)]
    values
        .iter()
        .map(|v| {
            if v.is_finite() {
                (v / max).clamp(0.0, 1.0) as f32
            } else {
                0.0
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn computes_cpu_percent_like_docker_stats() {
        let previous = CpuSample {
            total_usage: 1_000,
            system_usage: 100_000,
        };
        let current = CpuSample {
            total_usage: 6_000,
            system_usage: 200_000,
        };
        // 5 000 / 100 000 = 5 % of the machine; × 4 cores = 20 % of one core.
        assert!((cpu_percent(previous, current, 4) - 20.0).abs() < 1e-9);
        assert!((cpu_percent(previous, current, 0) - 5.0).abs() < 1e-9);
    }

    #[test]
    fn idle_or_first_sample_is_zero() {
        let sample = CpuSample {
            total_usage: 10,
            system_usage: 10,
        };
        assert!(cpu_percent(sample, sample, 8).abs() < f64::EPSILON);
        assert!(cpu_percent(CpuSample::default(), sample, 8) > 0.0);
        // Counters going backwards (container restarted) never produce a negative value.
        assert!(cpu_percent(sample, CpuSample::default(), 8).abs() < f64::EPSILON);
    }

    #[test]
    fn subtracts_page_cache() {
        let v2 = HashMap::from([("inactive_file".to_owned(), 300)]);
        let v1 = HashMap::from([("total_inactive_file".to_owned(), 100)]);
        assert_eq!(memory_usage(1_000, Some(&v2)), 700);
        assert_eq!(memory_usage(1_000, Some(&v1)), 900);
        assert_eq!(memory_usage(1_000, None), 1_000);
        assert_eq!(memory_usage(100, Some(&v2)), 100);
    }

    #[test]
    fn sums_block_io_for_both_cgroup_versions() {
        let v1 = [("Read", 10), ("Write", 5), ("Sync", 99), ("Total", 15)];
        let v2 = [("read", 7), ("write", 3), ("read", 1)];
        assert_eq!(block_io(v1), (10, 5));
        assert_eq!(block_io(v2), (8, 3));
        assert_eq!(block_io([]), (0, 0));
    }

    #[test]
    fn keeps_a_bounded_history_with_rates() {
        let mut history = StatsHistory::default();
        assert!(history.rates().is_none());
        let sample = |rx: u64, write: u64| Stats {
            cpu_percent: 2.0,
            mem_usage: 100,
            net_rx: rx,
            blk_write: write,
            ..Stats::default()
        };
        history.push(0.0, sample(1_000, 0));
        history.push(2.0, sample(3_000, 4_096));
        let rates = history.rates().unwrap_or_default();
        assert!((rates.net_rx - 1_000.0).abs() < 1e-9);
        assert!((rates.blk_write - 2_048.0).abs() < 1e-9);
        // Restarted container: counters reset, rate is 0 not negative.
        history.push(4.0, sample(10, 0));
        assert!(history.rates().is_some_and(|r| r.net_rx == 0.0));
        for i in 0..100 {
            history.push(f64::from(i) + 10.0, sample(0, 0));
        }
        assert_eq!(history.len(), HISTORY_LEN);
        assert_eq!(history.cpu().len(), HISTORY_LEN);
        assert_eq!(history.memory()[0], 100.0);
    }

    #[test]
    fn scales_sparklines_with_a_floor() {
        assert_eq!(sparkline(&[0.0, 50.0, 100.0], 10.0), [0.0, 0.5, 1.0]);
        // Idle: 1 % against a 10 % floor stays low.
        assert_eq!(sparkline(&[1.0], 10.0), [0.1]);
        assert_eq!(sparkline(&[f64::NAN, 0.0], 0.0), [0.0, 0.0]);
        assert!(sparkline(&[], 1.0).is_empty());
    }
}

//! CPU and memory figures computed the way `docker stats` does.

use std::collections::HashMap;

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
}

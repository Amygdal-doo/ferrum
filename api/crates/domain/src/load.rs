//! Load-test model: run limits, result tallies and latency statistics
//! (nearest-rank percentiles, linear histogram).

use std::collections::BTreeMap;

/// Hard cap on requests per run.
pub const MAX_LOAD_REQUESTS: usize = 10_000;
/// Hard cap on concurrent requests.
pub const MAX_LOAD_CONCURRENCY: usize = 100;
/// Hard cap on the duration limit.
pub const MAX_LOAD_DURATION_SECS: u64 = 600;

/// What to run. Build with [`LoadConfig::new`], which applies the caps.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LoadConfig {
    pub total: usize,
    pub concurrency: usize,
    /// Stop starting new requests after this many seconds (`None`: no limit).
    pub duration_secs: Option<u64>,
}

impl LoadConfig {
    /// Clamps every value into its allowed range (at least 1 request and
    /// 1 worker; concurrency never exceeds the total).
    #[must_use]
    pub fn new(total: usize, concurrency: usize, duration_secs: Option<u64>) -> Self {
        let total = total.clamp(1, MAX_LOAD_REQUESTS);
        Self {
            total,
            concurrency: concurrency.clamp(1, MAX_LOAD_CONCURRENCY).min(total),
            duration_secs: duration_secs
                .filter(|s| *s > 0)
                .map(|s| s.min(MAX_LOAD_DURATION_SECS)),
        }
    }

    /// Parses form text; `Err` explains the first invalid field. Values above
    /// the caps are rejected rather than silently clamped.
    pub fn parse(total: &str, concurrency: &str, duration: &str) -> Result<Self, String> {
        let number = |text: &str, what: &str, max: u64| -> Result<u64, String> {
            let n: u64 = text
                .trim()
                .parse()
                .map_err(|_| format!("{what} must be a whole number"))?;
            if n == 0 || n > max {
                return Err(format!("{what} must be between 1 and {max}"));
            }
            Ok(n)
        };
        let total = number(total, "Requests", MAX_LOAD_REQUESTS as u64)?;
        let concurrency = number(concurrency, "Concurrency", MAX_LOAD_CONCURRENCY as u64)?;
        let duration = if duration.trim().is_empty() {
            None
        } else {
            Some(number(duration, "Duration", MAX_LOAD_DURATION_SECS)?)
        };
        let to_usize = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
        Ok(Self::new(to_usize(total), to_usize(concurrency), duration))
    }
}

/// How one request of a load test ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadSample {
    Response { status: u16, latency_us: u64 },
    Error { kind: String, latency_us: u64 },
}

/// Running tally of a load test.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadTally {
    /// Responses with a status below 400.
    pub ok: usize,
    /// Responses with a status of 400 or more.
    pub http_errors: usize,
    /// No response (connect failure, timeout, …).
    pub failures: usize,
    pub by_status: BTreeMap<u16, usize>,
    pub by_error: BTreeMap<String, usize>,
    /// Latency of every response (errors excluded), microseconds.
    pub latencies_us: Vec<u64>,
    pub elapsed_ms: u64,
}

impl LoadTally {
    pub fn record(&mut self, sample: LoadSample) {
        match sample {
            LoadSample::Response { status, latency_us } => {
                if status < 400 {
                    self.ok += 1;
                } else {
                    self.http_errors += 1;
                }
                *self.by_status.entry(status).or_default() += 1;
                self.latencies_us.push(latency_us);
            }
            LoadSample::Error { kind, .. } => {
                self.failures += 1;
                *self.by_error.entry(kind).or_default() += 1;
            }
        }
    }

    #[must_use]
    pub fn completed(&self) -> usize {
        self.ok + self.http_errors + self.failures
    }

    /// Completed requests per second of elapsed time.
    #[must_use]
    pub fn requests_per_sec(&self) -> f64 {
        if self.elapsed_ms == 0 {
            return 0.0;
        }
        self.completed() as f64 * 1000.0 / self.elapsed_ms as f64
    }

    #[must_use]
    pub fn latency(&self) -> Option<LatencyStats> {
        LatencyStats::from_samples(&self.latencies_us)
    }
}

/// Latency summary in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LatencyStats {
    pub count: usize,
    pub min: u64,
    pub avg: u64,
    pub p50: u64,
    pub p90: u64,
    pub p99: u64,
    pub max: u64,
}

impl LatencyStats {
    /// `None` when there are no samples.
    #[must_use]
    pub fn from_samples(samples: &[u64]) -> Option<Self> {
        let mut sorted = samples.to_vec();
        sorted.sort_unstable();
        let (&min, &max) = (sorted.first()?, sorted.last()?);
        let sum: u128 = sorted.iter().map(|&s| u128::from(s)).sum();
        let avg = u64::try_from(sum / sorted.len() as u128).unwrap_or(u64::MAX);
        Some(Self {
            count: sorted.len(),
            min,
            avg,
            p50: percentile(&sorted, 50.0),
            p90: percentile(&sorted, 90.0),
            p99: percentile(&sorted, 99.0),
            max,
        })
    }
}

/// Nearest-rank percentile of an ascending slice (0 when empty).
#[must_use]
pub fn percentile(sorted: &[u64], pct: f64) -> u64 {
    if sorted.is_empty() {
        return 0;
    }
    let pct = pct.clamp(0.0, 100.0);
    let rank = (pct / 100.0 * sorted.len() as f64).ceil() as usize;
    sorted[rank.clamp(1, sorted.len()) - 1]
}

/// One histogram bar: samples in `[from, to)` (the last bar includes `to`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Bucket {
    pub from: u64,
    pub to: u64,
    pub count: usize,
}

/// Splits `[min, max]` of `samples` into `buckets` equal-width bars.
#[must_use]
pub fn histogram(samples: &[u64], buckets: usize) -> Vec<Bucket> {
    let (Some(&min), Some(&max)) = (samples.iter().min(), samples.iter().max()) else {
        return Vec::new();
    };
    let buckets = buckets.max(1) as u64;
    let width = ((max - min) / buckets + 1).max(1);
    let n = ((max - min) / width + 1).min(buckets);
    let mut out: Vec<Bucket> = (0..n)
        .map(|i| Bucket {
            from: min + i * width,
            to: min + (i + 1) * width,
            count: 0,
        })
        .collect();
    let last = out.len().saturating_sub(1);
    for &s in samples {
        let i = usize::try_from((s - min) / width).unwrap_or(usize::MAX);
        if let Some(b) = out.get_mut(i.min(last)) {
            b.count += 1;
        }
    }
    out
}

/// `850 µs`, `12.3 ms`, `1.25 s`.
#[must_use]
pub fn format_latency_us(us: u64) -> String {
    if us < 1_000 {
        format!("{us} µs")
    } else if us < 10_000_000 {
        format!("{:.1} ms", us as f64 / 1_000.0)
    } else {
        format!("{:.2} s", us as f64 / 1_000_000.0)
    }
}

/// Whether `url` targets this machine (`localhost`, `*.localhost`,
/// `127.x.x.x`, `::1`). Variables must already be resolved.
#[must_use]
pub fn is_local_url(url: &str) -> bool {
    let url = url.trim();
    let rest = url.split_once("://").map_or(url, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or_default();
    let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    let host = if let Some(v6) = host_port.strip_prefix('[') {
        v6.split(']').next().unwrap_or_default()
    } else {
        host_port.split(':').next().unwrap_or_default()
    };
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    host == "localhost"
        || host.ends_with(".localhost")
        || host == "::1"
        || host.strip_prefix("127.").is_some_and(|r| {
            r.split('.').count() == 3 && r.split('.').all(|p| p.parse::<u8>().is_ok())
        })
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;

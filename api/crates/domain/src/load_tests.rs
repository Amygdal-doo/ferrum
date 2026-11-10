use super::*;

#[test]
fn config_clamps_to_caps() {
    let c = LoadConfig::new(50_000, 1_000, Some(10_000));
    assert_eq!(
        c,
        LoadConfig {
            total: MAX_LOAD_REQUESTS,
            concurrency: MAX_LOAD_CONCURRENCY,
            duration_secs: Some(MAX_LOAD_DURATION_SECS)
        }
    );
    let c = LoadConfig::new(0, 0, Some(0));
    assert_eq!((c.total, c.concurrency, c.duration_secs), (1, 1, None));
    // Concurrency never exceeds the request count.
    assert_eq!(LoadConfig::new(5, 50, None).concurrency, 5);
}

#[test]
fn config_parse_rejects_out_of_range() {
    assert_eq!(
        LoadConfig::parse(" 200 ", "10", ""),
        Ok(LoadConfig::new(200, 10, None))
    );
    assert_eq!(
        LoadConfig::parse("200", "10", "30").map(|c| c.duration_secs),
        Ok(Some(30))
    );
    assert_eq!(
        LoadConfig::parse("10001", "1", ""),
        Err("Requests must be between 1 and 10000".into())
    );
    assert_eq!(
        LoadConfig::parse("10", "101", ""),
        Err("Concurrency must be between 1 and 100".into())
    );
    assert_eq!(
        LoadConfig::parse("abc", "1", ""),
        Err("Requests must be a whole number".into())
    );
    assert!(LoadConfig::parse("10", "0", "").is_err());
    assert!(LoadConfig::parse("10", "1", "-5").is_err());
}

#[test]
fn nearest_rank_percentiles() {
    let sorted: Vec<u64> = (1..=100).collect();
    assert_eq!(percentile(&sorted, 50.0), 50);
    assert_eq!(percentile(&sorted, 90.0), 90);
    assert_eq!(percentile(&sorted, 99.0), 99);
    assert_eq!(percentile(&sorted, 100.0), 100);
    assert_eq!(percentile(&sorted, 0.0), 1);
    assert_eq!(percentile(&[7], 99.0), 7);
    assert_eq!(percentile(&[], 50.0), 0);
    // 10 samples: p90 is the 9th, p99 the 10th.
    let ten: Vec<u64> = (1..=10).map(|n| n * 10).collect();
    assert_eq!(percentile(&ten, 90.0), 90);
    assert_eq!(percentile(&ten, 99.0), 100);
    assert_eq!(percentile(&ten, 50.0), 50);
}

#[test]
fn latency_stats_from_unsorted_samples() {
    let stats = LatencyStats::from_samples(&[30, 10, 20, 40]).expect("stats");
    assert_eq!(
        stats,
        LatencyStats {
            count: 4,
            min: 10,
            avg: 25,
            p50: 20,
            p90: 40,
            p99: 40,
            max: 40
        }
    );
    assert!(LatencyStats::from_samples(&[]).is_none());
}

#[test]
fn tally_counts_by_status_and_error() {
    let mut t = LoadTally::default();
    for status in [200, 200, 201, 404, 500] {
        t.record(LoadSample::Response {
            status,
            latency_us: 1_000,
        });
    }
    t.record(LoadSample::Error {
        kind: "connection failed".into(),
        latency_us: 5,
    });
    assert_eq!(
        (t.ok, t.http_errors, t.failures, t.completed()),
        (3, 2, 1, 6)
    );
    assert_eq!(t.by_status.get(&200), Some(&2));
    assert_eq!(t.by_status.get(&500), Some(&1));
    assert_eq!(t.by_error.get("connection failed"), Some(&1));
    assert_eq!(t.latencies_us.len(), 5);
    assert!(t.requests_per_sec().abs() < f64::EPSILON);
    t.elapsed_ms = 2_000;
    assert!((t.requests_per_sec() - 3.0).abs() < 1e-9);
    assert_eq!(t.latency().map(|l| l.max), Some(1_000));
}

#[test]
fn histogram_buckets_cover_every_sample() {
    let samples: Vec<u64> = (0..=100).collect();
    let h = histogram(&samples, 10);
    assert_eq!(h.len(), 10);
    assert_eq!(h.iter().map(|b| b.count).sum::<usize>(), samples.len());
    assert_eq!(h[0].from, 0);
    assert!(h.windows(2).all(|w| w[0].to == w[1].from));
    assert!(h.last().is_some_and(|b| b.to > 100));

    let same = histogram(&[5, 5, 5], 10);
    assert_eq!(
        same,
        [Bucket {
            from: 5,
            to: 6,
            count: 3
        }]
    );
    assert!(histogram(&[], 10).is_empty());
    let narrow = histogram(&[0, 3], 10);
    assert_eq!(narrow.len(), 4);
    assert_eq!((narrow[0].count, narrow[3].count), (1, 1));
}

#[test]
fn local_hosts() {
    for url in [
        "http://localhost:3000/x",
        "localhost",
        "https://api.localhost/",
        "http://127.0.0.1/",
        "127.1.2.3:80",
        "http://[::1]:8080/",
        "http://user:pw@LOCALHOST./",
    ] {
        assert!(is_local_url(url), "{url}");
    }
    for url in [
        "https://example.com",
        "http://127.0.0.1.evil.com/",
        "http://10.0.0.1/",
        "http://localhost.example.com/",
        "",
    ] {
        assert!(!is_local_url(url), "{url}");
    }
}

#[test]
fn latency_formatting() {
    assert_eq!(format_latency_us(850), "850 µs");
    assert_eq!(format_latency_us(12_345), "12.3 ms");
    assert_eq!(format_latency_us(9_999_999), "10000.0 ms");
    assert_eq!(format_latency_us(12_500_000), "12.50 s");
}

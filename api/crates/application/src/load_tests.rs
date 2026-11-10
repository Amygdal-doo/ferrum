use super::*;
use ferrumapi_domain::mock::{MockResponse, MockRoute};
use ferrumapi_domain::{HttpMethod, HttpRequest, VariableSet};
use ferrumapi_server::MockServer;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn ok(latency_us: u64) -> LoadSample {
    LoadSample::Response {
        status: 200,
        latency_us,
    }
}

/// Runs `run_load` with a fake exec that sleeps `ms` and tracks peak concurrency.
async fn fake_run(
    config: LoadConfig,
    ms: u64,
    cancel: &CancellationToken,
) -> (Vec<LoadEvent>, usize, usize) {
    let in_flight = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let calls = Arc::new(AtomicUsize::new(0));
    let mut events = Vec::new();
    run_load(
        config,
        cancel,
        || {
            let (in_flight, peak, calls) = (in_flight.clone(), peak.clone(), calls.clone());
            async move {
                calls.fetch_add(1, Ordering::SeqCst);
                let now = in_flight.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(now, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(ms)).await;
                in_flight.fetch_sub(1, Ordering::SeqCst);
                Some(ok(ms * 1000))
            }
        },
        |e| events.push(e),
    )
    .await;
    (
        events,
        peak.load(Ordering::SeqCst),
        calls.load(Ordering::SeqCst),
    )
}

fn done(events: &[LoadEvent]) -> (&LoadTally, LoadEnd) {
    match events.last() {
        Some(LoadEvent::Done(t, end)) => (t, *end),
        other => panic!("expected Done, got {other:?}"),
    }
}

#[tokio::test]
async fn sends_exactly_total_with_bounded_concurrency() {
    let cancel = CancellationToken::new();
    let (events, peak, calls) = fake_run(LoadConfig::new(40, 4, None), 5, &cancel).await;
    let (tally, end) = done(&events);
    assert_eq!((calls, tally.ok, end), (40, 40, LoadEnd::Completed));
    assert!((2..=4).contains(&peak), "peak {peak}");
    assert_eq!(
        events
            .iter()
            .filter(|e| matches!(e, LoadEvent::Done(..)))
            .count(),
        1
    );
    assert!(tally.elapsed_ms > 0);
}

#[tokio::test]
async fn progress_is_reported_while_running() {
    let cancel = CancellationToken::new();
    let (events, _, _) = fake_run(LoadConfig::new(20, 1, None), 25, &cancel).await;
    let progress: Vec<usize> = events
        .iter()
        .filter_map(|e| match e {
            LoadEvent::Progress(t) => Some(t.completed()),
            LoadEvent::Done(..) => None,
        })
        .collect();
    assert!(!progress.is_empty());
    assert!(progress.windows(2).all(|w| w[0] <= w[1]));
}

#[tokio::test]
async fn cancel_stops_starting_new_requests() {
    let cancel = CancellationToken::new();
    let stopper = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(60)).await;
        stopper.cancel();
    });
    let (events, _, calls) = fake_run(LoadConfig::new(1_000, 2, None), 10, &cancel).await;
    let (tally, end) = done(&events);
    assert_eq!(end, LoadEnd::Cancelled);
    assert!(calls < 100, "calls {calls}");
    assert!(tally.completed() <= calls);
}

#[tokio::test]
async fn duration_limit_ends_the_run() {
    let cancel = CancellationToken::new();
    let started = Instant::now();
    let (events, _, _) = fake_run(LoadConfig::new(10_000, 1, Some(1)), 20, &cancel).await;
    let (tally, end) = done(&events);
    assert_eq!(end, LoadEnd::TimeLimit);
    assert!(tally.completed() < 10_000);
    assert!(started.elapsed() < Duration::from_secs(3));
}

fn prepared(url: &str, base: &str) -> Prepared {
    let mut variables = VariableSet::new();
    variables.insert("base", base);
    Prepared {
        request: HttpRequest::new(HttpMethod::Get, url),
        variables,
    }
}

fn mock_route(path: &str, status: u16) -> MockRoute {
    MockRoute {
        name: path.into(),
        method: HttpMethod::Get,
        path: path.into(),
        response: MockResponse {
            status,
            ..MockResponse::default()
        },
    }
}

/// Runs `future` on the services runtime and waits for it.
fn on_services<T: Send + 'static>(
    svc: &AppServices,
    future: impl Future<Output = T> + Send + 'static,
) -> T {
    tokio::runtime::Builder::new_current_thread()
        .build()
        .expect("rt")
        .block_on(svc.spawn(future))
        .expect("task")
}

fn services() -> (AppServices, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("ferrumapi-load-{}", uuid::Uuid::new_v4()));
    (AppServices::start(&dir).expect("services"), dir)
}

#[test]
fn load_test_against_a_local_server() {
    let (svc, dir) = services();
    let runner = svc.clone();
    let (tally, end) = on_services(&svc, async move {
        let (log, _rx) = tokio::sync::mpsc::unbounded_channel();
        let routes = vec![mock_route("/ping", 200), mock_route("/boom", 503)];
        let server = MockServer::start(0, routes, log).await.expect("mock");
        let base = server.base_url();
        let cancel = CancellationToken::new();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        runner
            .run_load_test(
                prepared("{{base}}/ping", &base),
                LoadConfig::new(60, 6, None),
                cancel.clone(),
                tx,
            )
            .await;
        let mut last = None;
        while let Ok(e) = rx.try_recv() {
            last = Some(e);
        }
        let Some(LoadEvent::Done(ok_tally, end)) = last else {
            panic!("no Done event");
        };

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        runner
            .run_load_test(
                prepared("{{base}}/boom", &base),
                LoadConfig::new(10, 3, None),
                cancel,
                tx,
            )
            .await;
        let mut boom = None;
        while let Ok(e) = rx.try_recv() {
            boom = Some(e);
        }
        let Some(LoadEvent::Done(boom, _)) = boom else {
            panic!("no Done event");
        };
        assert_eq!((boom.ok, boom.http_errors), (0, 10));
        assert_eq!(boom.by_status.get(&503), Some(&10));
        server.shutdown().await;
        (ok_tally, end)
    });
    assert_eq!(end, LoadEnd::Completed);
    assert_eq!((tally.ok, tally.http_errors, tally.failures), (60, 0, 0));
    assert_eq!(tally.by_status.get(&200), Some(&60));
    let stats = tally.latency().expect("latency");
    assert_eq!(stats.count, 60);
    assert!(stats.min <= stats.p50 && stats.p50 <= stats.p99 && stats.p99 <= stats.max);
    drop(svc);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn unreachable_target_counts_failures() {
    let (svc, dir) = services();
    let runner = svc.clone();
    let tally = on_services(&svc, async move {
        // Bind then drop to get a port nothing listens on.
        let port = ferrumapi_server::bind_local(0)
            .await
            .and_then(|l| {
                l.local_addr()
                    .map_err(|e| ferrumapi_server::ServerError::Io(e.to_string()))
            })
            .expect("port")
            .port();
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let base = format!("http://127.0.0.1:{port}");
        runner
            .run_load_test(
                prepared("{{base}}/", &base),
                LoadConfig::new(5, 5, None),
                CancellationToken::new(),
                tx,
            )
            .await;
        let mut last = None;
        while let Ok(e) = rx.try_recv() {
            last = Some(e);
        }
        match last {
            Some(LoadEvent::Done(t, _)) => t,
            other => panic!("expected Done, got {other:?}"),
        }
    });
    assert_eq!(tally.failures, 5);
    assert_eq!(tally.by_error.get("connection failed"), Some(&5));
    assert!(tally.latency().is_none());
    drop(svc);
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn warns_for_remote_hosts_only() {
    assert!(load_target_warning(&prepared("{{base}}/x", "http://127.0.0.1:8080")).is_none());
    assert!(load_target_warning(&prepared("http://localhost:3000", "")).is_none());
    assert!(load_target_warning(&prepared("{{base}}/x", "https://api.example.com")).is_some());
}

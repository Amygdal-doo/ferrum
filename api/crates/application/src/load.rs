//! Load test: sends one prepared request `total` times with up to
//! `concurrency` in flight, through the same HTTP engine and variable
//! substitution as Send (no cookie jar, no history). Progress is reported
//! about five times a second.

use crate::AppServices;
use crate::send::Prepared;
use ferrumapi_domain::ExecError;
use ferrumapi_domain::load::{LoadConfig, LoadSample, LoadTally, is_local_url};
use ferrumapi_http::CancellationToken;
use futures::StreamExt as _;
use std::future::Future;
use std::time::{Duration, Instant};
use tokio::sync::mpsc::UnboundedSender;

const PROGRESS_EVERY: Duration = Duration::from_millis(200);

/// Why a load test ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LoadEnd {
    Completed,
    Cancelled,
    /// The duration limit passed before every request was sent.
    TimeLimit,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadEvent {
    Progress(LoadTally),
    Done(LoadTally, LoadEnd),
}

/// Warning shown before load-testing a host that is not this machine.
#[must_use]
pub fn load_target_warning(prepared: &Prepared) -> Option<String> {
    let url = prepared.variables.resolve(&prepared.request.url);
    (!is_local_url(&url)).then(|| {
        "This request targets a host that is not localhost. Only load-test servers you own or are allowed to test — this can look like an attack and may get you rate-limited or blocked.".to_owned()
    })
}

/// Runs `exec` `config.total` times with at most `config.concurrency` in
/// flight. `exec` returning `None` means it was cancelled (not counted).
/// No new request starts after `cancel` fires or the duration limit
/// passes. Always ends with exactly one [`LoadEvent::Done`].
pub async fn run_load<F, Fut>(
    config: LoadConfig,
    cancel: &CancellationToken,
    exec: F,
    mut report: impl FnMut(LoadEvent),
) where
    F: Fn() -> Fut,
    Fut: Future<Output = Option<LoadSample>>,
{
    let started = Instant::now();
    let deadline = config
        .duration_secs
        .map(|s| started + Duration::from_secs(s));
    let past_deadline = move || deadline.is_some_and(|d| Instant::now() >= d);
    let mut requests = futures::stream::iter(0..config.total)
        .take_while(|_| std::future::ready(!cancel.is_cancelled() && !past_deadline()))
        .map(|_| exec())
        .buffer_unordered(config.concurrency.max(1));
    let mut tally = LoadTally::default();
    let mut tick = tokio::time::interval(PROGRESS_EVERY);
    tick.tick().await;
    loop {
        tokio::select! {
            next = requests.next() => match next {
                Some(Some(sample)) => tally.record(sample),
                Some(None) => {}
                None => break,
            },
            _ = tick.tick() => {
                tally.elapsed_ms = elapsed_ms(started);
                report(LoadEvent::Progress(tally.clone()));
            }
        }
    }
    tally.elapsed_ms = elapsed_ms(started);
    let end = if cancel.is_cancelled() {
        LoadEnd::Cancelled
    } else if tally.completed() < config.total {
        LoadEnd::TimeLimit
    } else {
        LoadEnd::Completed
    };
    report(LoadEvent::Done(tally, end));
}

fn elapsed_ms(started: Instant) -> u64 {
    u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX)
}

/// Groups errors under a stable label (details vary per request).
fn error_kind(e: &ExecError) -> String {
    match e {
        ExecError::Timeout(_) => "timeout".into(),
        ExecError::Connect(_) => "connection failed".into(),
        ExecError::Tls(_) => "TLS error".into(),
        ExecError::Redirect => "too many redirects".into(),
        ExecError::Other(_) => "request failed".into(),
        other => other.to_string(),
    }
}

impl AppServices {
    /// Load-tests `prepared` (auth already resolved), streaming progress to
    /// `events` (a closed receiver does not stop the run; `cancel` does).
    pub async fn run_load_test(
        &self,
        prepared: Prepared,
        config: LoadConfig,
        cancel: CancellationToken,
        events: UnboundedSender<LoadEvent>,
    ) {
        tracing::info!(
            total = config.total,
            concurrency = config.concurrency,
            "load test started"
        );
        let exec = || {
            let cancel = cancel.clone();
            let prepared = &prepared;
            async move {
                let started = Instant::now();
                let result = self
                    .http
                    .execute(&prepared.request, &prepared.variables, cancel)
                    .await;
                let latency_us = u64::try_from(started.elapsed().as_micros()).unwrap_or(u64::MAX);
                match result {
                    Ok(r) => Some(LoadSample::Response {
                        status: r.status,
                        latency_us,
                    }),
                    Err(ExecError::Cancelled) => None,
                    Err(e) => Some(LoadSample::Error {
                        kind: error_kind(&e),
                        latency_us,
                    }),
                }
            }
        };
        run_load(config, &cancel, exec, |event| {
            if let LoadEvent::Done(tally, end) = &event {
                tracing::info!(completed = tally.completed(), ?end, "load test finished");
            }
            let _ = events.send(event);
        })
        .await;
    }
}

#[cfg(test)]
#[path = "load_tests.rs"]
mod tests;

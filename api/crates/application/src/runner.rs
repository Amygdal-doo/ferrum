//! Collection runner: sends every request of a collection or folder in tree
//! order, one at a time, reporting progress as [`RunEvent`]s. A response
//! with a 2xx status passes (there are no test scripts yet). Runs use the
//! cookie jar but are not recorded in history.

use crate::AppServices;
use crate::send::{Prepared, prepare};
use ferrumapi_domain::{
    Collection, CollectionId, Environment, ExecError, HttpMethod, ItemId, RequestOrigin,
};
use ferrumapi_http::CancellationToken;
use std::future::Future;
use tokio::sync::mpsc::UnboundedSender;

/// One request of a run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunItem {
    pub origin: RequestOrigin,
    pub name: String,
    pub method: HttpMethod,
    pub prepared: Prepared,
}

/// How one request of a run ended.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunOutcome {
    /// A response arrived; `passed` when its status is 2xx.
    Response {
        status: u16,
        time_ms: u64,
        passed: bool,
    },
    /// No response (invalid URL, connection refused, timeout, …).
    Error(String),
}

impl RunOutcome {
    #[must_use]
    pub const fn passed(&self) -> bool {
        matches!(self, Self::Response { passed: true, .. })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RunEvent {
    /// Request `index` is being sent.
    Started(usize),
    Finished(usize, RunOutcome),
    /// The run ended; `stopped` when it was cancelled before the last request.
    Done {
        stopped: bool,
    },
}

/// The requests under `folder` (whole collection when `None`), each with
/// inherited auth and variables resolved as if sent from its saved tab.
#[must_use]
pub fn plan_run(
    collection: CollectionId,
    folder: Option<ItemId>,
    collections: &[Collection],
    globals: &Environment,
    environment: Option<&Environment>,
) -> Vec<RunItem> {
    let Some(c) = collections.iter().find(|c| c.id == collection) else {
        return Vec::new();
    };
    c.requests_under(folder)
        .into_iter()
        .map(|saved| {
            let origin = RequestOrigin {
                collection,
                item: saved.id,
            };
            RunItem {
                origin,
                name: saved.name.clone(),
                method: saved.request.method,
                prepared: prepare(
                    &saved.request,
                    Some(origin),
                    collections,
                    globals,
                    environment,
                ),
            }
        })
        .collect()
}

/// Runs `count` steps strictly in order through `exec`. `exec` returning
/// `None` means the step was cancelled. Stops before the next step once
/// `cancel` fires. Always ends with exactly one [`RunEvent::Done`].
pub async fn run_sequence<F, Fut>(
    count: usize,
    cancel: &CancellationToken,
    mut exec: F,
    mut report: impl FnMut(RunEvent),
) where
    F: FnMut(usize) -> Fut,
    Fut: Future<Output = Option<RunOutcome>>,
{
    for index in 0..count {
        if cancel.is_cancelled() {
            report(RunEvent::Done { stopped: true });
            return;
        }
        report(RunEvent::Started(index));
        match exec(index).await {
            Some(outcome) => report(RunEvent::Finished(index, outcome)),
            None => {
                report(RunEvent::Done { stopped: true });
                return;
            }
        }
    }
    report(RunEvent::Done { stopped: false });
}

impl AppServices {
    /// Runs `items` in order, sending progress to `events` (a closed
    /// receiver does not stop the run; `cancel` does).
    pub async fn run_collection(
        &self,
        items: Vec<RunItem>,
        cancel: CancellationToken,
        events: UnboundedSender<RunEvent>,
    ) {
        tracing::info!(count = items.len(), "collection run started");
        let exec = |index: usize| {
            let prepared = items.get(index).map(|item| item.prepared.clone());
            let cancel = cancel.clone();
            async move {
                let prepared = prepared?;
                match self.execute(prepared, cancel).await {
                    Ok(r) => Some(RunOutcome::Response {
                        status: r.status,
                        time_ms: r.time_ms,
                        passed: (200..300).contains(&r.status),
                    }),
                    Err(ExecError::Cancelled) => None,
                    Err(e) => Some(RunOutcome::Error(e.to_string())),
                }
            }
        };
        run_sequence(items.len(), &cancel, exec, |event| {
            if let RunEvent::Done { stopped } = event {
                tracing::info!(stopped, "collection run finished");
            }
            // The UI may have gone away; the run itself is unaffected.
            let _ = events.send(event);
        })
        .await;
    }
}

#[cfg(test)]
#[path = "runner_tests.rs"]
mod tests;

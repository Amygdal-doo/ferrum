//! Query execution supervisor: stamps events, tracks cancellation handles and
//! records every run in the query history.

use crate::ConnectionService;
use chrono::{DateTime, Utc};
use ferrumdata_db_driver_api::{CancellationHandle, EVENT_CHANNEL_CAPACITY};
use ferrumdata_domain::{
    DbError, ExecutionEnvelope, ExecutionId, HistoryEntry, ProfileId, QueryEvent, QueryRequest,
};
use ferrumdata_persistence::AppStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Instant;
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct QueryService {
    connections: ConnectionService,
    running: Arc<Mutex<HashMap<ExecutionId, CancellationHandle>>>,
}

/// What the supervisor needs to write the history entry when a run ends.
struct HistoryRecorder {
    store: AppStore,
    profile_id: ProfileId,
    sql: String,
    executed_at: DateTime<Utc>,
    started: Instant,
}

impl HistoryRecorder {
    /// Best effort: a failed write is logged and never fails the run.
    async fn record(self, terminal: &QueryEvent) {
        let Some(entry) = HistoryEntry::from_terminal(
            self.profile_id,
            self.sql,
            self.executed_at,
            self.started.elapsed(),
            terminal,
        ) else {
            return;
        };
        if let Err(e) = self.store.record_history(&entry).await {
            tracing::warn!(error = %e, "failed to record query history");
        }
    }
}

impl QueryService {
    #[must_use]
    pub fn new(connections: ConnectionService) -> Self {
        Self {
            connections,
            running: Arc::default(),
        }
    }

    /// Starts a run and returns a bounded stream of stamped events. The
    /// stream always ends with exactly one terminal event, which is sent
    /// after the run has been written to the history.
    pub async fn execute(
        &self,
        request: QueryRequest,
    ) -> Result<mpsc::Receiver<ExecutionEnvelope>, DbError> {
        let execution_id = request.execution_id;
        let recorder = HistoryRecorder {
            store: self.connections.store().clone(),
            profile_id: request.profile_id,
            sql: request.sql.clone(),
            executed_at: Utc::now(),
            started: Instant::now(),
        };
        let started = async {
            let conn = self
                .connections
                .ensure_connected(request.profile_id)
                .await?;
            conn.execute(request).await
        }
        .await;
        let execution = match started {
            Ok(execution) => execution,
            Err(e) => {
                // Failed before the driver produced any event (e.g. no connection).
                recorder.record(&QueryEvent::Failed(e.clone())).await;
                return Err(e);
            }
        };
        self.lock()
            .insert(execution_id, execution.cancellation.clone());

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let running = self.running.clone();
        let mut events = execution.events;
        let cancellation = execution.cancellation;
        tokio::spawn(async move {
            let mut seq = 0u64;
            let mut recorder = Some(recorder);
            let mut saw_terminal = false;
            while let Some(event) = events.recv().await {
                seq += 1;
                saw_terminal = event.is_terminal();
                if saw_terminal && let Some(r) = recorder.take() {
                    r.record(&event).await;
                }
                if tx
                    .send(ExecutionEnvelope {
                        execution_id,
                        seq,
                        event,
                    })
                    .await
                    .is_err()
                {
                    // Consumer is gone (tab closed or run superseded): stop server work.
                    cancellation.cancel();
                    if let Some(r) = recorder.take() {
                        let elapsed = r.started.elapsed();
                        r.record(&QueryEvent::Cancelled { elapsed }).await;
                    }
                    break;
                }
                if saw_terminal {
                    break;
                }
            }
            if !saw_terminal {
                let event = QueryEvent::Failed(DbError::new(
                    ferrumdata_domain::DbErrorKind::Internal,
                    "Driver ended the stream without a result",
                ));
                if let Some(r) = recorder.take() {
                    r.record(&event).await;
                }
                let _ = tx
                    .send(ExecutionEnvelope {
                        execution_id,
                        seq: seq + 1,
                        event,
                    })
                    .await;
            }
            running
                .lock()
                .unwrap_or_else(PoisonError::into_inner)
                .remove(&execution_id);
        });
        Ok(rx)
    }

    /// Best-effort cancel. Returns `false` if the run already finished.
    pub fn cancel(&self, execution_id: ExecutionId) -> bool {
        match self.lock().get(&execution_id) {
            Some(handle) => {
                handle.cancel();
                true
            }
            None => false,
        }
    }

    #[must_use]
    pub fn running_count(&self) -> usize {
        self.lock().len()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, HashMap<ExecutionId, CancellationHandle>> {
        self.running.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

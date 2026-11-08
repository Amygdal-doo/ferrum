//! Query execution supervisor: stamps events, tracks cancellation handles.

use crate::ConnectionService;
use ferrum_db_driver_api::{CancellationHandle, EVENT_CHANNEL_CAPACITY};
use ferrum_domain::{DbError, ExecutionEnvelope, ExecutionId, QueryEvent, QueryRequest};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};
use tokio::sync::mpsc;

#[derive(Clone)]
pub struct QueryService {
    connections: ConnectionService,
    running: Arc<Mutex<HashMap<ExecutionId, CancellationHandle>>>,
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
    /// stream always ends with exactly one terminal event.
    pub async fn execute(
        &self,
        request: QueryRequest,
    ) -> Result<mpsc::Receiver<ExecutionEnvelope>, DbError> {
        let execution_id = request.execution_id;
        let conn = self
            .connections
            .ensure_connected(request.profile_id)
            .await?;
        let execution = conn.execute(request).await?;
        self.lock()
            .insert(execution_id, execution.cancellation.clone());

        let (tx, rx) = mpsc::channel(EVENT_CHANNEL_CAPACITY);
        let running = self.running.clone();
        let mut events = execution.events;
        let cancellation = execution.cancellation;
        tokio::spawn(async move {
            let mut seq = 0u64;
            let mut saw_terminal = false;
            while let Some(event) = events.recv().await {
                seq += 1;
                saw_terminal = event.is_terminal();
                if tx
                    .send(ExecutionEnvelope {
                        execution_id,
                        seq,
                        event,
                    })
                    .await
                    .is_err()
                {
                    // Consumer is gone (tab closed): stop server work.
                    cancellation.cancel();
                    break;
                }
                if saw_terminal {
                    break;
                }
            }
            if !saw_terminal {
                let event = QueryEvent::Failed(DbError::new(
                    ferrum_domain::DbErrorKind::Internal,
                    "Driver ended the stream without a result",
                ));
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

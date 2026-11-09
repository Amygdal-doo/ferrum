//! Contract every database adapter implements.
//!
//! Adapters depend on this crate and `ferrumdata-domain`, never on UI code.
//! See `docs/DRIVER_CONTRACT.md` for the behavioural rules tested per driver.

pub mod documents;

use async_trait::async_trait;
use ferrumdata_domain::{
    CapabilitySet, DatabaseEngine, DbError, QueryEvent, QueryRequest, ResolvedConnectionConfig,
    TableInfo,
};
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub type DbResult<T> = Result<T, DbError>;

/// Max undelivered event messages per execution. Drivers stop fetching from
/// the server while the channel is full, which bounds memory to roughly
/// `EVENT_CHANNEL_CAPACITY * batch_rows` rows in flight.
pub const EVENT_CHANNEL_CAPACITY: usize = 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServerInfo {
    pub version: String,
}

/// Entry point for an engine.
#[async_trait]
pub trait DatabaseDriver: Send + Sync {
    fn engine(&self) -> DatabaseEngine;
    fn capabilities(&self) -> CapabilitySet;

    /// Opens a connection pool for the profile and verifies it with a round trip.
    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>>;
}

/// A live, pooled connection to one database.
#[async_trait]
pub trait DbConnection: Send + Sync {
    async fn server_info(&self) -> DbResult<ServerInfo>;

    /// User-visible tables, views and similar objects, ordered by schema then name.
    /// System schemas are excluded.
    async fn list_tables(&self) -> DbResult<Vec<TableInfo>>;

    /// Starts executing one statement. The returned handle streams events
    /// over a bounded channel; the driver keeps its session leased until the
    /// stream ends, is cancelled, or the receiver is dropped.
    async fn execute(&self, request: QueryRequest) -> DbResult<QueryExecution>;

    /// Closes the pool. In-flight executions are cancelled.
    async fn close(&self);
}

/// Handle to a running execution.
pub struct QueryExecution {
    pub events: mpsc::Receiver<QueryEvent>,
    pub cancellation: CancellationHandle,
}

/// Requests best-effort cancellation. The driver acknowledges with a
/// `QueryEvent::Cancelled` and is responsible for server-side cancel and for
/// not returning a possibly-busy session to the pool.
#[derive(Debug, Clone, Default)]
pub struct CancellationHandle(CancellationToken);

impl CancellationHandle {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.cancel();
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.0.is_cancelled()
    }

    /// Completes when cancellation is requested.
    pub async fn cancelled(&self) {
        self.0.cancelled().await;
    }
}

/// Creates the bounded event channel drivers must use.
#[must_use]
pub fn event_channel() -> (mpsc::Sender<QueryEvent>, mpsc::Receiver<QueryEvent>) {
    mpsc::channel(EVENT_CHANNEL_CAPACITY)
}

//! Contract every database adapter implements.
//!
//! Adapters depend on this crate and `ferrumdata-domain`, never on UI code.
//! See `docs/DRIVER_CONTRACT.md` for the behavioural rules tested per driver.

pub mod documents;

use async_trait::async_trait;
use ferrumdata_domain::{
    CapabilitySet, DatabaseEngine, DbError, KeyScanPage, KeyValueView, QueryEvent, QueryRequest,
    ResolvedConnectionConfig, TableDetails, TableInfo, TableRef,
};
use std::sync::Arc;
use std::time::Duration;
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

    /// Columns and indexes of one table, from the catalog. SQL engines only;
    /// document engines keep the default `Unsupported` error.
    async fn describe_table(&self, table: &TableRef) -> DbResult<TableDetails> {
        let _ = table;
        Err(DbError::unsupported(
            "Column and index details are not available for this engine",
        ))
    }

    /// Key browser operations for key-value engines (Redis); `None` for
    /// engines browsed by table.
    fn key_value(&self) -> Option<&dyn KeyValueStore> {
        None
    }

    /// Starts executing one statement. The returned handle streams events
    /// over a bounded channel; the driver keeps its session leased until the
    /// stream ends, is cancelled, or the receiver is dropped.
    async fn execute(&self, request: QueryRequest) -> DbResult<QueryExecution>;

    /// Closes the pool. In-flight executions are cancelled.
    async fn close(&self);
}

/// Key-level browsing and editing for key-value engines. Writes are refused
/// with a `Query` error on read-only profiles.
#[async_trait]
pub trait KeyValueStore: Send + Sync {
    /// Number of keys in the selected database (`DBSIZE`).
    async fn key_count(&self) -> DbResult<u64>;

    /// One page of keys matching the glob `pattern`, with type and TTL,
    /// continuing from `cursor` (`0` starts over). Uses incremental `SCAN`,
    /// never `KEYS`; a page may hold fewer than `page_size` keys.
    async fn scan_keys(
        &self,
        pattern: &str,
        cursor: u64,
        page_size: usize,
    ) -> DbResult<KeyScanPage>;

    /// Type, TTL, memory usage and up to `max_items` elements of one key.
    async fn inspect_key(&self, key: &str, max_items: usize) -> DbResult<KeyValueView>;

    /// Replaces a string value, keeping the key's TTL.
    async fn set_string(&self, key: &str, value: &str) -> DbResult<()>;

    /// Sets a TTL, or removes it when `ttl` is `None`. Returns false if nothing
    /// changed (no such key, or no TTL to remove).
    async fn set_ttl(&self, key: &str, ttl: Option<Duration>) -> DbResult<bool>;

    /// Deletes a key. Returns false if it did not exist.
    async fn delete_key(&self, key: &str) -> DbResult<bool>;
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

//! Redis / Valkey adapter (redis-rs over Tokio, TLS via rustls).
//!
//! Two surfaces: the key browser ([`KeyValueStore`]: `SCAN` pages, per-type
//! value views, string/TTL/delete edits) on one shared multiplexed
//! connection, and the command console ([`DbConnection::execute`]) where each
//! run gets its own connection. Read-only profiles only run commands on an
//! explicit allow-list (see [`command`]) and refuse key edits.

pub mod command;
mod error;
mod execute;
pub mod inspect;
mod options;
pub mod reply;

use async_trait::async_trait;
use ferrumdata_db_driver_api::{
    CancellationHandle, DatabaseDriver, DbConnection, DbResult, KeyValueStore, QueryExecution,
    ServerInfo, event_channel,
};
use ferrumdata_domain::{
    CapabilitySet, DatabaseEngine, DbError, DbErrorKind, KeyScanPage, KeyValueView, QueryRequest,
    ResolvedConnectionConfig, Secret, TableInfo,
};
use redis::aio::MultiplexedConnection;
use redis::{AsyncConnectionConfig, Client, RedisError};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

pub use command::writes;
pub use error::map_redis_error;
pub use options::connection_info;

/// Response timeout for key browser calls (the console uses the query timeout).
const BROWSE_TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Debug, Default, Clone, Copy)]
pub struct RedisDriver;

impl RedisDriver {
    pub const CAPABILITIES: CapabilitySet = CapabilitySet {
        transactions: false,
        // Client-side: the run's dedicated connection is dropped.
        query_cancel: true,
        schema_introspection: true,
        edit_rows: false,
        multi_result: false,
        explain: false,
        parameters: false,
        import: false,
        ddl: false,
    };
}

/// rustls needs a process-wide crypto provider when more than one is
/// compiled in (the workspace builds both ring and aws-lc-rs).
fn install_crypto_provider() {
    // Err means a provider is already installed, which is fine.
    let _ = rustls::crypto::ring::default_provider().install_default();
}

#[async_trait]
impl DatabaseDriver for RedisDriver {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::Redis
    }

    fn capabilities(&self) -> CapabilitySet {
        Self::CAPABILITIES
    }

    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>> {
        let secret = config.password.clone().unwrap_or_else(|| Secret::new(""));
        let info = connection_info(config)?;
        if options::uses_tls(&info) {
            install_crypto_provider();
        }
        let client = Client::open(info).map_err(|e| map_redis_error(&e, &secret))?;
        let conn = RedisConnection {
            client,
            secret,
            read_only: config.profile.read_only,
            connect_timeout: config.profile.connect_timeout(),
            browse: Mutex::new(None),
            shutdown: CancellationToken::new(),
        };
        // Proves reachability and credentials (AUTH/SELECT run on connect).
        let info = conn.server_info().await?;
        tracing::info!(profile = %config.profile.id, version = %info.version, "redis connected");
        Ok(Arc::new(conn))
    }
}

pub struct RedisConnection {
    client: Client,
    secret: Secret,
    read_only: bool,
    connect_timeout: Duration,
    /// Shared key-browser connection, reopened after a connection error.
    browse: Mutex<Option<MultiplexedConnection>>,
    shutdown: CancellationToken,
}

impl RedisConnection {
    fn map(&self, e: &RedisError) -> DbError {
        map_redis_error(e, &self.secret)
    }

    async fn conn(&self) -> DbResult<MultiplexedConnection> {
        let mut slot = self.browse.lock().await;
        if let Some(conn) = slot.as_ref() {
            return Ok(conn.clone());
        }
        let config = AsyncConnectionConfig::new()
            .set_connection_timeout(Some(self.connect_timeout))
            .set_response_timeout(Some(BROWSE_TIMEOUT));
        let conn = self
            .client
            .get_multiplexed_async_connection_with_config(&config)
            .await
            .map_err(|e| self.map(&e))?;
        *slot = Some(conn.clone());
        Ok(conn)
    }

    /// Runs a browser call; a broken connection is dropped so the next call reconnects.
    async fn browse<T, F>(&self, f: impl FnOnce(MultiplexedConnection) -> F) -> DbResult<T>
    where
        F: Future<Output = redis::RedisResult<T>>,
    {
        let conn = self.conn().await?;
        match f(conn).await {
            Ok(v) => Ok(v),
            Err(e) => {
                if e.is_io_error() || e.is_connection_dropped() || e.is_unrecoverable_error() {
                    *self.browse.lock().await = None;
                }
                Err(self.map(&e))
            }
        }
    }

    fn refuse_if_read_only(&self, what: &str) -> DbResult<()> {
        if self.read_only {
            return Err(DbError::new(
                DbErrorKind::Query,
                format!("This profile is read-only: {what} is disabled"),
            ));
        }
        Ok(())
    }
}

/// `redis_version` (or `valkey_version`) from `INFO server`.
fn server_label(info: &str) -> String {
    let field = |name: &str| {
        info.lines()
            .find_map(|l| l.trim().strip_prefix(name)?.strip_prefix(':'))
            .map(str::to_owned)
    };
    match (field("valkey_version"), field("redis_version")) {
        (Some(v), _) => format!("Valkey {v}"),
        (None, Some(v)) => format!("Redis {v}"),
        (None, None) => "Redis".into(),
    }
}

#[async_trait]
impl DbConnection for RedisConnection {
    async fn server_info(&self) -> DbResult<ServerInfo> {
        let text: String = self
            .browse(
                |mut c| async move { redis::cmd("INFO").arg("server").query_async(&mut c).await },
            )
            .await?;
        Ok(ServerInfo {
            version: server_label(&text),
        })
    }

    /// Redis has no tables: the explorer lists keys through [`KeyValueStore`].
    async fn list_tables(&self) -> DbResult<Vec<TableInfo>> {
        Ok(Vec::new())
    }

    fn key_value(&self) -> Option<&dyn KeyValueStore> {
        Some(self)
    }

    async fn execute(&self, request: QueryRequest) -> DbResult<QueryExecution> {
        let (tx, events) = event_channel();
        let cancellation = CancellationHandle::new();
        let ctx = execute::ExecContext {
            client: self.client.clone(),
            secret: self.secret.clone(),
            read_only: self.read_only,
            connect_timeout: self.connect_timeout,
            cancel: cancellation.clone(),
            shutdown: self.shutdown.clone(),
        };
        tokio::spawn(execute::run(ctx, request, tx));
        Ok(QueryExecution {
            events,
            cancellation,
        })
    }

    async fn close(&self) {
        self.shutdown.cancel();
        // Dropping the last handle closes the multiplexed connection.
        self.browse.lock().await.take();
    }
}

#[async_trait]
impl KeyValueStore for RedisConnection {
    async fn key_count(&self) -> DbResult<u64> {
        self.browse(|mut c| async move { redis::cmd("DBSIZE").query_async(&mut c).await })
            .await
    }

    async fn scan_keys(
        &self,
        pattern: &str,
        cursor: u64,
        page_size: usize,
    ) -> DbResult<KeyScanPage> {
        let pattern = if pattern.trim().is_empty() {
            "*".to_owned()
        } else {
            pattern.to_owned()
        };
        self.browse(|mut c| async move { inspect::scan(&mut c, &pattern, cursor, page_size).await })
            .await
    }

    async fn inspect_key(&self, key: &str, max_items: usize) -> DbResult<KeyValueView> {
        let key = key.to_owned();
        self.browse(|mut c| async move { inspect::inspect(&mut c, &key, max_items).await })
            .await
    }

    async fn set_string(&self, key: &str, value: &str) -> DbResult<()> {
        self.refuse_if_read_only("editing values")?;
        let (key, value) = (key.to_owned(), value.to_owned());
        self.browse(|mut c| async move {
            // KEEPTTL (Redis 6+): editing the value must not drop the expiry.
            redis::cmd("SET")
                .arg(&key)
                .arg(&value)
                .arg("KEEPTTL")
                .query_async::<()>(&mut c)
                .await
        })
        .await
    }

    async fn set_ttl(&self, key: &str, ttl: Option<Duration>) -> DbResult<bool> {
        self.refuse_if_read_only("changing TTLs")?;
        let key = key.to_owned();
        self.browse(|mut c| async move {
            let changed: i64 = match ttl {
                Some(ttl) => {
                    let ms = u64::try_from(ttl.as_millis()).unwrap_or(u64::MAX).max(1);
                    redis::cmd("PEXPIRE")
                        .arg(&key)
                        .arg(ms)
                        .query_async(&mut c)
                        .await?
                }
                None => redis::cmd("PERSIST").arg(&key).query_async(&mut c).await?,
            };
            Ok(changed == 1)
        })
        .await
    }

    async fn delete_key(&self, key: &str) -> DbResult<bool> {
        self.refuse_if_read_only("deleting keys")?;
        let key = key.to_owned();
        self.browse(|mut c| async move {
            let removed: i64 = redis::cmd("DEL").arg(&key).query_async(&mut c).await?;
            Ok(removed == 1)
        })
        .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn server_label_prefers_valkey() {
        assert_eq!(
            server_label("# Server\r\nredis_version:7.4.1\r\nredis_mode:standalone\r\n"),
            "Redis 7.4.1"
        );
        assert_eq!(
            server_label("redis_version:7.2.4\r\nvalkey_version:8.0.1\r\n"),
            "Valkey 8.0.1"
        );
        assert_eq!(server_label(""), "Redis");
    }
}

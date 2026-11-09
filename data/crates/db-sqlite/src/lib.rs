//! SQLite adapter (SQLx). Profiles point at a database file on local disk.

mod decode;
mod error;
mod execute;
mod introspect;
mod options;

use async_trait::async_trait;
use ferrumdata_db_driver_api::{
    CancellationHandle, DatabaseDriver, DbConnection, DbResult, QueryExecution, ServerInfo,
    event_channel,
};
use ferrumdata_domain::{
    CapabilitySet, DatabaseEngine, QueryRequest, ResolvedConnectionConfig, TableDetails, TableInfo,
    TableRef,
};
use sqlx::SqlitePool;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub use error::map_sqlx_error;
pub use options::connect_options;

/// SQLite serialises writers; a few readers in parallel are plenty.
const MAX_POOL_CONNECTIONS: u32 = 4;

#[derive(Debug, Default, Clone, Copy)]
pub struct SqliteDriver;

impl SqliteDriver {
    pub const CAPABILITIES: CapabilitySet = CapabilitySet {
        transactions: true,
        // Via SQLite's progress handler (SQLITE_INTERRUPT).
        query_cancel: true,
        schema_introspection: true,
        edit_rows: false,
        multi_result: false,
        explain: false,
        parameters: false,
        import: false,
        ddl: true,
    };
}

#[async_trait]
impl DatabaseDriver for SqliteDriver {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::Sqlite
    }

    fn capabilities(&self) -> CapabilitySet {
        Self::CAPABILITIES
    }

    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>> {
        let options = connect_options(config)?;
        let pool = SqlitePoolOptions::new()
            .max_connections(MAX_POOL_CONNECTIONS)
            .acquire_timeout(config.profile.connect_timeout())
            .test_before_acquire(true)
            .connect_with(options)
            .await
            .map_err(|e| map_sqlx_error(&e))?;
        let conn = SqliteConnection {
            pool,
            shutdown: CancellationToken::new(),
        };
        let info = conn.server_info().await?;
        tracing::info!(profile = %config.profile.id, version = %info.version, "sqlite opened");
        Ok(Arc::new(conn))
    }
}

pub struct SqliteConnection {
    pool: SqlitePool,
    shutdown: CancellationToken,
}

#[async_trait]
impl DbConnection for SqliteConnection {
    async fn server_info(&self) -> DbResult<ServerInfo> {
        let version: String = sqlx::query_scalar("SELECT sqlite_version()")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| map_sqlx_error(&e))?;
        Ok(ServerInfo {
            version: format!("SQLite {version}"),
        })
    }

    async fn list_tables(&self) -> DbResult<Vec<TableInfo>> {
        introspect::list_tables(&self.pool).await
    }

    async fn describe_table(&self, table: &TableRef) -> DbResult<TableDetails> {
        introspect::describe_table(&self.pool, table).await
    }

    async fn execute(&self, request: QueryRequest) -> DbResult<QueryExecution> {
        let (tx, events) = event_channel();
        let cancellation = CancellationHandle::new();
        let ctx = execute::ExecContext {
            pool: self.pool.clone(),
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
        self.pool.close().await;
    }
}

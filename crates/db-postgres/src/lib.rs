//! PostgreSQL adapter (SQLx).

mod decode;
mod error;
mod execute;
mod introspect;
mod numeric;
mod options;

use async_trait::async_trait;
use ferrum_db_driver_api::{
    CancellationHandle, DatabaseDriver, DbConnection, DbResult, QueryExecution, ServerInfo,
    event_channel,
};
use ferrum_domain::{
    CapabilitySet, DatabaseEngine, QueryRequest, ResolvedConnectionConfig, Secret, TableInfo,
};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub use error::map_sqlx_error;
pub use options::{PgOptions, connect_options};

/// Default pool size per profile; conservative to respect server limits.
const MAX_POOL_CONNECTIONS: u32 = 4;

#[derive(Debug, Default, Clone, Copy)]
pub struct PostgresDriver;

impl PostgresDriver {
    pub const CAPABILITIES: CapabilitySet = CapabilitySet {
        transactions: true,
        query_cancel: true,
        schema_introspection: true,
        edit_rows: false, // IDE-014
        multi_result: false,
        explain: false,
        parameters: false,
        import: false,
        ddl: true,
    };
}

#[async_trait]
impl DatabaseDriver for PostgresDriver {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::Postgres
    }

    fn capabilities(&self) -> CapabilitySet {
        Self::CAPABILITIES
    }

    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>> {
        let secret = config.password.clone().unwrap_or_else(|| Secret::new(""));
        let options = connect_options(config)?;
        let pool = PgPoolOptions::new()
            .max_connections(MAX_POOL_CONNECTIONS)
            .acquire_timeout(config.profile.connect_timeout())
            .test_before_acquire(true)
            .connect_with(options.0.clone())
            .await
            .map_err(|e| map_sqlx_error(&e, &secret))?;
        let conn = PostgresConnection {
            pool,
            options,
            secret,
            shutdown: CancellationToken::new(),
        };
        let info = conn.server_info().await?;
        tracing::info!(profile = %config.profile.id, version = %info.version, "postgres connected");
        Ok(Arc::new(conn))
    }
}

pub struct PostgresConnection {
    pool: PgPool,
    /// Used to open a side connection for `pg_cancel_backend`.
    options: PgOptions,
    secret: Secret,
    shutdown: CancellationToken,
}

#[async_trait]
impl DbConnection for PostgresConnection {
    async fn server_info(&self) -> DbResult<ServerInfo> {
        let version: String = sqlx::query_scalar("SHOW server_version")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| map_sqlx_error(&e, &self.secret))?;
        Ok(ServerInfo {
            version: format!("PostgreSQL {version}"),
        })
    }

    async fn list_tables(&self) -> DbResult<Vec<TableInfo>> {
        introspect::list_tables(&self.pool, &self.secret).await
    }

    async fn execute(&self, request: QueryRequest) -> DbResult<QueryExecution> {
        let (tx, events) = event_channel();
        let cancellation = CancellationHandle::new();
        let ctx = execute::ExecContext {
            pool: self.pool.clone(),
            options: self.options.clone(),
            secret: self.secret.clone(),
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

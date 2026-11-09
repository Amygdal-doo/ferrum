//! MySQL / MariaDB adapter (SQLx).

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
    CapabilitySet, DatabaseEngine, QueryRequest, ResolvedConnectionConfig, Secret, TableInfo,
};
use sqlx::mysql::MySqlPoolOptions;
use sqlx::{Executor, MySqlPool};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

pub use error::map_sqlx_error;
pub use options::{MySqlOptions, connect_options};

/// Default pool size per profile; conservative to respect server limits.
const MAX_POOL_CONNECTIONS: u32 = 4;

#[derive(Debug, Default, Clone, Copy)]
pub struct MySqlDriver;

impl MySqlDriver {
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
impl DatabaseDriver for MySqlDriver {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::MySql
    }

    fn capabilities(&self) -> CapabilitySet {
        Self::CAPABILITIES
    }

    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>> {
        let secret = config.password.clone().unwrap_or_else(|| Secret::new(""));
        let options = connect_options(config)?;
        let mut pool = MySqlPoolOptions::new()
            .max_connections(MAX_POOL_CONNECTIONS)
            .acquire_timeout(config.profile.connect_timeout())
            .test_before_acquire(true);
        if config.profile.read_only {
            // Server-enforced default for every transaction in the session. Not a
            // security boundary (a session can override it); use read-only users.
            pool = pool.after_connect(|conn, _meta| {
                Box::pin(async move {
                    conn.execute("SET SESSION TRANSACTION READ ONLY").await?;
                    Ok(())
                })
            });
        }
        let pool = pool
            .connect_with(options.0.clone())
            .await
            .map_err(|e| map_sqlx_error(&e, &secret))?;
        let conn = MySqlConnection {
            pool,
            options,
            secret,
            shutdown: CancellationToken::new(),
        };
        let info = conn.server_info().await?;
        tracing::info!(profile = %config.profile.id, version = %info.version, "mysql connected");
        Ok(Arc::new(conn))
    }
}

pub struct MySqlConnection {
    pool: MySqlPool,
    /// Used to open a side connection for `KILL QUERY`.
    options: MySqlOptions,
    secret: Secret,
    shutdown: CancellationToken,
}

#[async_trait]
impl DbConnection for MySqlConnection {
    async fn server_info(&self) -> DbResult<ServerInfo> {
        let version: String = sqlx::query_scalar("SELECT VERSION()")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| map_sqlx_error(&e, &self.secret))?;
        Ok(ServerInfo {
            version: version_label(&version),
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

/// MariaDB reports itself through MySQL's `VERSION()`, e.g. `11.4.2-MariaDB`.
fn version_label(version: &str) -> String {
    if version.contains("MariaDB") {
        format!("MariaDB {version}")
    } else {
        format!("MySQL {version}")
    }
}

#[cfg(test)]
mod tests {
    use super::version_label;

    #[test]
    fn labels_mariadb_and_mysql() {
        assert_eq!(version_label("8.4.0"), "MySQL 8.4.0");
        assert_eq!(version_label("11.4.2-MariaDB"), "MariaDB 11.4.2-MariaDB");
    }
}

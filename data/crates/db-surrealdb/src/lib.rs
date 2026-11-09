//! SurrealDB adapter: remote WebSocket client (`ws://` / `wss://`), SurrealQL input.
//!
//! The SDK is built without embedded storage engines; FerrumData only talks to
//! running servers.

mod decode;
mod error;
mod execute;
mod guard;
mod introspect;
mod options;

use async_trait::async_trait;
use ferrumdata_db_driver_api::{
    CancellationHandle, DatabaseDriver, DbConnection, DbResult, QueryExecution, ServerInfo,
    event_channel,
};
use ferrumdata_domain::{
    CapabilitySet, DatabaseEngine, DbError, DbErrorKind, QueryRequest, ResolvedConnectionConfig,
    Secret, TableInfo,
};
use std::sync::Arc;
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::{Client, Ws, Wss};
use surrealdb::opt::auth::{Database, Namespace, Root};
use surrealdb::types::Value;
use tokio_util::sync::CancellationToken;

pub use error::map_surreal_error;
pub use guard::writes;
pub use options::{SurrealTarget, target};

#[derive(Debug, Default, Clone, Copy)]
pub struct SurrealDriver;

impl SurrealDriver {
    pub const CAPABILITIES: CapabilitySet = CapabilitySet {
        transactions: true,
        // Cancel stops reading and drops the result stream; over WebSocket the
        // server may still finish the statement (SurrealQL has no KILL QUERY).
        query_cancel: false,
        schema_introspection: true,
        edit_rows: false,
        multi_result: false,
        explain: true,
        parameters: false,
        import: false,
        ddl: true,
    };
}

#[async_trait]
impl DatabaseDriver for SurrealDriver {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::SurrealDb
    }

    fn capabilities(&self) -> CapabilitySet {
        Self::CAPABILITIES
    }

    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>> {
        let target = target(config)?;
        let secret = config.password.clone().unwrap_or_else(|| Secret::new(""));
        let db = tokio::time::timeout(config.profile.connect_timeout(), open(&target, &secret))
            .await
            .map_err(|_| {
                DbError::new(
                    DbErrorKind::Timeout,
                    format!("Timed out connecting to {}", target.url()),
                )
            })??;
        let conn = SurrealConnection {
            db,
            secret,
            read_only: config.profile.read_only,
            shutdown: CancellationToken::new(),
        };
        let info = conn.server_info().await?;
        tracing::info!(profile = %config.profile.id, version = %info.version, "surrealdb connected");
        Ok(Arc::new(conn))
    }
}

/// Connects, signs in and selects the namespace/database.
async fn open(target: &SurrealTarget, secret: &Secret) -> DbResult<Surreal<Client>> {
    let connect_err = |e: surrealdb::Error| map_surreal_error(&e, secret, false);
    let db = if target.tls {
        Surreal::new::<Wss>(target.address.clone()).await
    } else {
        Surreal::new::<Ws>(target.address.clone()).await
    }
    .map_err(connect_err)?;
    if let Some(username) = &target.username {
        signin(&db, target, username, secret).await?;
    }
    db.use_ns(target.namespace.clone())
        .use_db(target.database.clone())
        .await
        .map_err(connect_err)?;
    Ok(db)
}

/// Tries root, then database, then namespace users: the profile does not say
/// at which level the user is defined. The last error is reported.
async fn signin(
    db: &Surreal<Client>,
    target: &SurrealTarget,
    username: &str,
    secret: &Secret,
) -> DbResult<()> {
    let map = |e: surrealdb::Error| map_surreal_error(&e, secret, true);
    let root = db
        .signin(Root {
            username: username.to_owned(),
            password: target.password.clone(),
        })
        .await;
    let Err(root_err) = root else {
        return Ok(());
    };
    if !root_err.is_not_allowed() {
        return Err(map(root_err));
    }
    let database = db
        .signin(Database {
            namespace: target.namespace.clone(),
            database: target.database.clone(),
            username: username.to_owned(),
            password: target.password.clone(),
        })
        .await;
    if database.is_ok() {
        return Ok(());
    }
    db.signin(Namespace {
        namespace: target.namespace.clone(),
        username: username.to_owned(),
        password: target.password.clone(),
    })
    .await
    .map(|_| ())
    .map_err(map)
}

pub struct SurrealConnection {
    db: Surreal<Client>,
    secret: Secret,
    read_only: bool,
    shutdown: CancellationToken,
}

#[async_trait]
impl DbConnection for SurrealConnection {
    async fn server_info(&self) -> DbResult<ServerInfo> {
        let version = self
            .db
            .version()
            .await
            .map_err(|e| map_surreal_error(&e, &self.secret, false))?;
        Ok(ServerInfo {
            version: format!("SurrealDB {version}"),
        })
    }

    async fn list_tables(&self) -> DbResult<Vec<TableInfo>> {
        let map = |e: surrealdb::Error| map_surreal_error(&e, &self.secret, false);
        let mut response = self.db.query("INFO FOR DB").await.map_err(map)?;
        let info: Value = response.take(0).map_err(map)?;
        Ok(introspect::tables_from_info(info))
    }

    async fn execute(&self, request: QueryRequest) -> DbResult<QueryExecution> {
        let (tx, events) = event_channel();
        let cancellation = CancellationHandle::new();
        let ctx = execute::ExecContext {
            db: self.db.clone(),
            secret: self.secret.clone(),
            read_only: self.read_only,
            cancel: cancellation.clone(),
            shutdown: self.shutdown.clone(),
        };
        tokio::spawn(execute::run(ctx, request, tx));
        Ok(QueryExecution {
            events,
            cancellation,
        })
    }

    /// The WebSocket closes when the last client handle is dropped; in-flight
    /// executions are stopped here.
    async fn close(&self) {
        self.shutdown.cancel();
    }
}

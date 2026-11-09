//! MongoDB adapter (official driver).
//!
//! The editor accepts a mongo-shell subset (`db.users.find({…})`, `aggregate`,
//! `countDocuments`, writes…) or a JSON database command; see [`query`].
//! Documents are flattened to grid columns: top-level fields become columns,
//! nested values are rendered as Extended JSON.

mod decode;
mod error;
mod execute;
mod options;
pub mod query;
mod relaxed_json;

use async_trait::async_trait;
use bson::doc;
use ferrumdata_db_driver_api::{
    CancellationHandle, DatabaseDriver, DbConnection, DbResult, QueryExecution, ServerInfo,
    event_channel,
};
use ferrumdata_domain::{
    CapabilitySet, DatabaseEngine, DbError, DbErrorKind, QueryRequest, ResolvedConnectionConfig,
    Secret, TableInfo, TableKind, TableRef,
};
use futures::TryStreamExt;
use mongodb::results::CollectionType;
use mongodb::{Client, Database};
use std::sync::Arc;
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub use error::map_mongo_error;
pub use options::client_options;

/// Upper bound for `close` while the driver drains in-flight operations.
const SHUTDOWN_GRACE: Duration = Duration::from_secs(5);

/// True if the editor text parses as a query that may write (used by the
/// production confirmation). Unparseable text is not a write: it fails to run.
#[must_use]
pub fn is_write_query(text: &str) -> bool {
    query::parse(text).is_ok_and(|q| q.is_write())
}

#[derive(Debug, Default, Clone, Copy)]
pub struct MongoDriver;

impl MongoDriver {
    pub const CAPABILITIES: CapabilitySet = CapabilitySet {
        transactions: false,
        // Cursors are killed server-side; a running single operation is bounded by max_time.
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

#[async_trait]
impl DatabaseDriver for MongoDriver {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::MongoDb
    }

    fn capabilities(&self) -> CapabilitySet {
        Self::CAPABILITIES
    }

    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>> {
        let secret = config.password.clone().unwrap_or_else(|| Secret::new(""));
        let database = config
            .profile
            .database
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
            .ok_or_else(|| {
                DbError::new(DbErrorKind::Connection, "MongoDB requires a database name")
            })?
            .to_owned();
        let options = client_options(config, &secret).await?;
        let client = Client::with_options(options).map_err(|e| map_mongo_error(&e, &secret))?;
        let conn = MongoConnection {
            db: client.database(&database),
            client,
            secret,
            read_only: config.profile.read_only,
            shutdown: CancellationToken::new(),
        };
        // The driver connects lazily: this round trip proves reachability and credentials.
        let info = conn.server_info().await?;
        tracing::info!(profile = %config.profile.id, version = %info.version, "mongodb connected");
        Ok(Arc::new(conn))
    }
}

pub struct MongoConnection {
    client: Client,
    db: Database,
    secret: Secret,
    read_only: bool,
    shutdown: CancellationToken,
}

#[async_trait]
impl DbConnection for MongoConnection {
    async fn server_info(&self) -> DbResult<ServerInfo> {
        let reply = self
            .db
            .run_command(doc! { "buildInfo": 1 })
            .await
            .map_err(|e| map_mongo_error(&e, &self.secret))?;
        let version = reply.get_str("version").unwrap_or("unknown");
        Ok(ServerInfo {
            version: format!("MongoDB {version}"),
        })
    }

    async fn list_tables(&self) -> DbResult<Vec<TableInfo>> {
        let map = |e: mongodb::error::Error| map_mongo_error(&e, &self.secret);
        let specs: Vec<_> = self
            .db
            .list_collections()
            .await
            .map_err(map)?
            .try_collect()
            .await
            .map_err(map)?;
        let mut out: Vec<TableInfo> = specs
            .into_iter()
            .filter(|s| !s.name.starts_with("system."))
            .map(|s| TableInfo {
                kind: match s.collection_type {
                    CollectionType::View => TableKind::View,
                    _ => TableKind::Table,
                },
                // Views have no `_id` index, but documents still carry `_id`.
                primary_key: vec!["_id".to_owned()],
                table: TableRef {
                    schema: None,
                    name: s.name,
                },
                estimated_rows: None,
            })
            .collect();
        out.sort_by(|a, b| a.table.name.cmp(&b.table.name));
        Ok(out)
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

    async fn close(&self) {
        self.shutdown.cancel();
        if tokio::time::timeout(SHUTDOWN_GRACE, self.client.clone().shutdown())
            .await
            .is_err()
        {
            tracing::warn!("mongodb client did not shut down in time");
        }
    }
}

//! Ferrum domain model.
//!
//! Pure types shared by every layer. This crate must never depend on UI
//! frameworks or concrete database drivers.

pub mod connection;
pub mod error;
pub mod ids;
pub mod query;
pub mod schema;
pub mod secret;
pub mod settings;
pub mod value;

pub use connection::{
    CapabilitySet, ConnectionProfile, ConnectionState, DatabaseEngine, Endpoint, EnvironmentTag,
    ResolvedConnectionConfig, SecretRef, TlsMode,
};
pub use error::{DbError, DbErrorKind};
pub use ids::{EditorTabId, ExecutionId, ProfileId};
pub use query::{
    ExecutionEnvelope, QueryEvent, QueryLimits, QueryOutcome, QueryRequest, QueryState, RowBatch,
};
pub use schema::{TableInfo, TableKind, TableRef};
pub use secret::Secret;
pub use settings::{AppSettings, EditorTabState, LayoutSettings, Theme};
pub use value::{CellValue, ColumnMeta, LogicalType};

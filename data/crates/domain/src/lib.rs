//! FerrumData domain model.
//!
//! Pure types shared by every layer. This crate must never depend on UI
//! frameworks or concrete database drivers.

pub mod connection;
pub mod error;
pub mod history;
pub mod ids;
pub mod keyvalue;
pub mod literal;
pub mod query;
pub mod schema;
pub mod secret;
pub mod settings;
pub mod value;

pub use connection::{
    CapabilitySet, ConnectionProfile, ConnectionState, DatabaseEngine, Endpoint, EndpointKind,
    EnvironmentTag, ResolvedConnectionConfig, SecretRef, TlsMode,
};
pub use error::{DbError, DbErrorKind};
pub use history::{HistoryEntry, HistoryStatus};
pub use ids::{EditorTabId, ExecutionId, ProfileId};
pub use keyvalue::{KeyEntry, KeyKind, KeyScanPage, KeyTtl, KeyValueView};
pub use query::{
    ExecutionEnvelope, QueryEvent, QueryLimits, QueryOutcome, QueryRequest, QueryState, RowBatch,
};
pub use schema::{
    ColumnInfo, ForeignKeyInfo, IndexInfo, SchemaSnapshot, TableDetails, TableInfo, TableKind,
    TableRef, TableSchema, redis_quote,
};
pub use secret::Secret;
pub use settings::{AppSettings, EditorTabState, LayoutSettings, Theme};
pub use value::{CellValue, ColumnMeta, LogicalType};

//! Local application state (collections, environments, history, workspace and
//! settings) stored in a dedicated SQLite file.
//!
//! Records are JSON documents in TEXT columns. Unreadable records are logged
//! (ids only, never contents) and skipped or defaulted instead of failing the
//! whole load.

mod collections;
mod environments;
mod history;
mod workspace;

use chrono::Utc;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("local database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("local database migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("corrupt local record: {0}")]
    Corrupt(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

/// Handle to the app-state database. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    /// Opens (creating if needed) the database file and runs migrations.
    pub async fn open(path: &Path) -> StoreResult<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        Self::from_options(options).await
    }

    /// Private in-memory store (tests, or when no data dir is available).
    pub async fn in_memory() -> StoreResult<Self> {
        let options = SqliteConnectOptions::new()
            .in_memory(true)
            .foreign_keys(true);
        Self::from_options(options).await
    }

    async fn from_options(options: SqliteConnectOptions) -> StoreResult<Self> {
        // One connection: keeps in-memory databases coherent and writes serialized.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn to_json<T: serde::Serialize>(value: &T) -> StoreResult<String> {
    serde_json::to_string(value)
        .map_err(|e| StoreError::Corrupt(format!("unserializable value ({:?})", e.classify())))
}

#[cfg(test)]
mod test_util;

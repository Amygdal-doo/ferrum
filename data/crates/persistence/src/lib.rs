//! Local application state stored in a dedicated SQLite file.

use chrono::Utc;
use ferrumdata_domain::{AppSettings, ConnectionProfile, EditorTabState, ProfileId};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::{Row, SqlitePool};
use std::path::Path;

const SETTINGS_KEY: &str = "app";

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
pub struct AppStore {
    pool: SqlitePool,
}

impl AppStore {
    /// Opens (creating if needed) the database file and runs migrations.
    pub async fn open(path: &Path) -> StoreResult<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        Self::from_options(options).await
    }

    /// In-memory store for tests.
    pub async fn in_memory() -> StoreResult<Self> {
        let options = SqliteConnectOptions::new().in_memory(true);
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

    pub async fn load_settings(&self) -> StoreResult<AppSettings> {
        let row = sqlx::query("SELECT value FROM settings WHERE key = ?")
            .bind(SETTINGS_KEY)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(AppSettings::default());
        };
        let json: String = row.try_get("value")?;
        serde_json::from_str(&json).or_else(|e| {
            tracing::warn!(error = %e, "settings unreadable; using defaults");
            Ok(AppSettings::default())
        })
    }

    pub async fn save_settings(&self, settings: &AppSettings) -> StoreResult<()> {
        let json = to_json(settings)?;
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(SETTINGS_KEY)
        .bind(json)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn list_profiles(&self) -> StoreResult<Vec<ConnectionProfile>> {
        let rows = sqlx::query("SELECT data FROM connection_profiles ORDER BY created_at")
            .fetch_all(&self.pool)
            .await?;
        let mut profiles = Vec::with_capacity(rows.len());
        for row in rows {
            let json: String = row.try_get("data")?;
            match serde_json::from_str(&json) {
                Ok(p) => profiles.push(p),
                Err(e) => tracing::warn!(error = %e, "skipping unreadable connection profile"),
            }
        }
        Ok(profiles)
    }

    pub async fn upsert_profile(&self, profile: &ConnectionProfile) -> StoreResult<()> {
        let json = to_json(profile)?;
        let ts = now();
        sqlx::query(
            "INSERT INTO connection_profiles (id, data, created_at, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
        )
        .bind(profile.id.to_string())
        .bind(json)
        .bind(&ts)
        .bind(&ts)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_profile(&self, id: ProfileId) -> StoreResult<()> {
        sqlx::query("DELETE FROM connection_profiles WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn load_tabs(&self) -> StoreResult<Vec<EditorTabState>> {
        let rows = sqlx::query(
            "SELECT id, title, sql, profile_id, position, table_ref FROM editor_tabs ORDER BY position",
        )
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                let id: String = row.try_get("id")?;
                let profile: Option<String> = row.try_get("profile_id")?;
                let table: Option<String> = row.try_get("table_ref")?;
                Ok(EditorTabState {
                    id: id
                        .parse()
                        .map_err(|_| StoreError::Corrupt(format!("tab id {id}")))?,
                    title: row.try_get("title")?,
                    sql: row.try_get("sql")?,
                    profile_id: profile.and_then(|p| p.parse().ok()),
                    position: row.try_get("position")?,
                    table: table.and_then(|t| serde_json::from_str(&t).ok()),
                })
            })
            .collect()
    }

    /// Replaces the saved tab set atomically.
    pub async fn save_tabs(&self, tabs: &[EditorTabState]) -> StoreResult<()> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM editor_tabs")
            .execute(&mut *tx)
            .await?;
        let ts = now();
        for tab in tabs {
            sqlx::query(
                "INSERT INTO editor_tabs (id, title, sql, profile_id, position, table_ref, updated_at)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(tab.id.to_string())
            .bind(&tab.title)
            .bind(&tab.sql)
            .bind(tab.profile_id.map(|p| p.to_string()))
            .bind(tab.position)
            .bind(tab.table.as_ref().map(to_json).transpose()?)
            .bind(&ts)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        Ok(())
    }
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn to_json<T: serde::Serialize>(value: &T) -> StoreResult<String> {
    serde_json::to_string(value).map_err(|e| StoreError::Corrupt(e.to_string()))
}

#[cfg(test)]
mod tests;

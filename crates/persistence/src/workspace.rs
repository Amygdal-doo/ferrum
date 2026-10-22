//! Workspace state and app settings, stored as keyed JSON in `settings`.

use crate::{Store, StoreResult, now, to_json};
use ferrumapi_domain::{AppSettings, WorkspaceState};
use serde::Serialize;
use serde::de::DeserializeOwned;

const SETTINGS_KEY: &str = "app";
const WORKSPACE_KEY: &str = "workspace";

impl Store {
    /// Default (empty) state when nothing was saved yet or it is unreadable.
    pub async fn load_workspace(&self) -> StoreResult<WorkspaceState> {
        self.load_key(WORKSPACE_KEY).await
    }

    pub async fn save_workspace(&self, ws: &WorkspaceState) -> StoreResult<()> {
        self.save_key(WORKSPACE_KEY, ws).await
    }

    /// Defaults when nothing was saved yet or it is unreadable.
    pub async fn load_settings(&self) -> StoreResult<AppSettings> {
        self.load_key(SETTINGS_KEY).await
    }

    pub async fn save_settings(&self, s: &AppSettings) -> StoreResult<()> {
        self.save_key(SETTINGS_KEY, s).await
    }

    async fn load_key<T: DeserializeOwned + Default>(&self, key: &str) -> StoreResult<T> {
        let json: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        let Some(json) = json else {
            return Ok(T::default());
        };
        Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
            tracing::warn!(key, kind = ?e.classify(), line = e.line(), "stored value unreadable; using defaults");
            T::default()
        }))
    }

    async fn save_key<T: Serialize>(&self, key: &str, value: &T) -> StoreResult<()> {
        let json = to_json(value)?;
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(json)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "workspace_tests.rs"]
mod tests;

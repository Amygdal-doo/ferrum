//! Environments and the globals environment (row with the nil id).

use crate::{Store, StoreResult, now, to_json};
use ferrumapi_domain::{Environment, EnvironmentId};
use sqlx::Row;

impl Store {
    /// Ordered by name (case-insensitive). Never includes the globals
    /// environment. Unreadable rows are skipped.
    pub async fn list_environments(&self) -> StoreResult<Vec<Environment>> {
        let rows = sqlx::query(
            "SELECT id, data FROM environments WHERE id != ? ORDER BY name COLLATE NOCASE, id",
        )
        .bind(EnvironmentId::GLOBALS.to_string())
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("id")?;
            let json: String = row.try_get("data")?;
            match serde_json::from_str(&json) {
                Ok(e) => out.push(e),
                Err(e) => {
                    tracing::warn!(%id, kind = ?e.classify(), line = e.line(), "skipping unreadable environment")
                }
            }
        }
        Ok(out)
    }

    /// Insert or replace. Saving an environment with the globals id is the
    /// same as [`Store::save_globals`].
    pub async fn save_environment(&self, e: &Environment) -> StoreResult<()> {
        let json = to_json(e)?;
        let ts = now();
        sqlx::query(
            "INSERT INTO environments (id, name, data, created_at, updated_at) VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, data = excluded.data,
                 updated_at = excluded.updated_at",
        )
        .bind(e.id.to_string())
        .bind(&e.name)
        .bind(json)
        .bind(&ts)
        .bind(&ts)
        .execute(&self.pool)
        .await?;
        tracing::debug!(id = %e.id, vars = e.variables.len(), "environment saved");
        Ok(())
    }

    pub async fn delete_environment(&self, id: EnvironmentId) -> StoreResult<()> {
        sqlx::query("DELETE FROM environments WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// Returns `Environment::globals()` when nothing was saved yet (or the
    /// saved record is unreadable).
    pub async fn load_globals(&self) -> StoreResult<Environment> {
        let json: Option<String> = sqlx::query_scalar("SELECT data FROM environments WHERE id = ?")
            .bind(EnvironmentId::GLOBALS.to_string())
            .fetch_optional(&self.pool)
            .await?;
        let Some(json) = json else {
            return Ok(Environment::globals());
        };
        match serde_json::from_str::<Environment>(&json) {
            Ok(mut g) => {
                g.id = EnvironmentId::GLOBALS;
                Ok(g)
            }
            Err(e) => {
                tracing::warn!(kind = ?e.classify(), line = e.line(), "globals unreadable; using defaults");
                Ok(Environment::globals())
            }
        }
    }

    /// Saves the globals. The id is always forced to `EnvironmentId::GLOBALS`.
    pub async fn save_globals(&self, globals: &Environment) -> StoreResult<()> {
        let mut g = globals.clone();
        g.id = EnvironmentId::GLOBALS;
        self.save_environment(&g).await
    }
}

#[cfg(test)]
#[path = "environments_tests.rs"]
mod tests;

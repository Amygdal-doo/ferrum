//! Collections: one JSON row per collection holding the whole item tree.

use crate::{Store, StoreResult, now, to_json};
use ferrumapi_domain::{Collection, CollectionId};
use sqlx::Row;

impl Store {
    /// Ordered by name (case-insensitive). Unreadable rows are skipped.
    pub async fn list_collections(&self) -> StoreResult<Vec<Collection>> {
        let rows = sqlx::query(
            "SELECT id, project, data FROM collections ORDER BY name COLLATE NOCASE, id",
        )
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("id")?;
            let json: String = row.try_get("data")?;
            let project: Option<String> = row.try_get("project")?;
            match serde_json::from_str::<Collection>(&json) {
                // The column is the source of truth for the project.
                Ok(c) => out.push(Collection { project, ..c }),
                Err(e) => {
                    tracing::warn!(%id, kind = ?e.classify(), line = e.line(), "skipping unreadable collection")
                }
            }
        }
        Ok(out)
    }

    /// Insert or replace (whole tree).
    pub async fn save_collection(&self, c: &Collection) -> StoreResult<()> {
        let json = to_json(c)?;
        let ts = now();
        sqlx::query(
            "INSERT INTO collections (id, name, project, data, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET name = excluded.name, project = excluded.project,
                 data = excluded.data, updated_at = excluded.updated_at",
        )
        .bind(c.id.to_string())
        .bind(&c.name)
        .bind(c.project.as_deref())
        .bind(json)
        .bind(&ts)
        .bind(&ts)
        .execute(&self.pool)
        .await?;
        tracing::debug!(id = %c.id, "collection saved");
        Ok(())
    }

    pub async fn delete_collection(&self, id: CollectionId) -> StoreResult<()> {
        sqlx::query("DELETE FROM collections WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

#[cfg(test)]
#[path = "collections_tests.rs"]
mod tests;

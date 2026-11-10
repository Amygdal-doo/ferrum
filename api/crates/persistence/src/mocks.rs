//! Mock server definitions, one JSON row per collection.

use crate::{Store, StoreResult, now, to_json};
use ferrumapi_domain::CollectionId;
use ferrumapi_domain::mock::CollectionMocks;
use sqlx::Row;

impl Store {
    /// The collection's mocks; defaults (port 4010, no responses) when none
    /// are stored or the row is unreadable.
    pub async fn load_mocks(&self, collection: CollectionId) -> StoreResult<CollectionMocks> {
        let row = sqlx::query("SELECT data FROM mocks WHERE collection_id = ?")
            .bind(collection.to_string())
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(CollectionMocks::default());
        };
        let json: String = row.try_get("data")?;
        Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
            tracing::warn!(%collection, kind = ?e.classify(), "unreadable mocks, using defaults");
            CollectionMocks::default()
        }))
    }

    /// Insert or replace. The collection must already be saved.
    pub async fn save_mocks(
        &self,
        collection: CollectionId,
        mocks: &CollectionMocks,
    ) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO mocks (collection_id, data, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(collection_id) DO UPDATE SET data = excluded.data,
                 updated_at = excluded.updated_at",
        )
        .bind(collection.to_string())
        .bind(to_json(mocks)?)
        .bind(now())
        .execute(&self.pool)
        .await?;
        tracing::debug!(%collection, count = mocks.responses.len(), "mocks saved");
        Ok(())
    }
}

#[cfg(test)]
#[path = "mocks_tests.rs"]
mod tests;

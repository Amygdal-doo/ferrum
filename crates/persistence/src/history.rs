//! Sent-request history, ordered by `sent_at` (integer millis column).

use crate::{Store, StoreResult, to_json};
use ferrumapi_domain::{HistoryEntry, HistoryId};
use sqlx::Row;

impl Store {
    /// Inserts (or replaces, by id) and prunes so at most `keep` newest
    /// entries remain. `keep == 0` leaves the history empty.
    pub async fn add_history(&self, entry: &HistoryEntry, keep: usize) -> StoreResult<()> {
        let json = to_json(entry)?;
        let keep = i64::try_from(keep).unwrap_or(i64::MAX);
        let mut tx = self.pool.begin().await?;
        sqlx::query(
            "INSERT INTO history (id, sent_at, sent_at_ms, data) VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET sent_at = excluded.sent_at,
                 sent_at_ms = excluded.sent_at_ms, data = excluded.data",
        )
        .bind(entry.id.to_string())
        .bind(entry.sent_at.to_rfc3339())
        .bind(entry.sent_at.timestamp_millis())
        .bind(json)
        .execute(&mut *tx)
        .await?;
        let pruned = sqlx::query(
            "DELETE FROM history WHERE id NOT IN (
                 SELECT id FROM history ORDER BY sent_at_ms DESC, rowid DESC LIMIT ?)",
        )
        .bind(keep)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        tracing::debug!(id = %entry.id, pruned, "history entry added");
        Ok(())
    }

    /// Newest first. Unreadable rows are skipped.
    pub async fn list_history(&self, limit: usize) -> StoreResult<Vec<HistoryEntry>> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let rows = sqlx::query(
            "SELECT id, data FROM history ORDER BY sent_at_ms DESC, rowid DESC LIMIT ?",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut out = Vec::with_capacity(rows.len());
        for row in rows {
            let id: String = row.try_get("id")?;
            let json: String = row.try_get("data")?;
            match serde_json::from_str(&json) {
                Ok(h) => out.push(h),
                Err(e) => {
                    tracing::warn!(%id, kind = ?e.classify(), line = e.line(), "skipping unreadable history entry")
                }
            }
        }
        Ok(out)
    }

    pub async fn delete_history(&self, id: HistoryId) -> StoreResult<()> {
        sqlx::query("DELETE FROM history WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    pub async fn clear_history(&self) -> StoreResult<()> {
        let n = sqlx::query("DELETE FROM history")
            .execute(&self.pool)
            .await?
            .rows_affected();
        tracing::debug!(removed = n, "history cleared");
        Ok(())
    }
}

#[cfg(test)]
#[path = "history_tests.rs"]
mod tests;

//! Query history records (table `query_history`).

use crate::{AppStore, StoreError, StoreResult};
use chrono::{DateTime, Utc};
use ferrumdata_domain::{HistoryEntry, HistoryStatus, ProfileId};
use sqlx::Row;
use sqlx::sqlite::SqliteRow;
use std::time::Duration;

/// Entries kept per connection; older ones are pruned on insert.
pub const HISTORY_RETENTION: usize = 2_000;

/// Filter for [`AppStore::list_history`]. Newest entries first.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HistoryQuery {
    /// `None` lists every connection.
    pub profile_id: Option<ProfileId>,
    /// Case-insensitive (ASCII) substring of the query text; empty matches all.
    pub search: String,
    pub limit: usize,
}

impl AppStore {
    /// Stores one entry and prunes the connection's history to
    /// [`HISTORY_RETENTION`]. Returns the new entry id.
    pub async fn record_history(&self, entry: &HistoryEntry) -> StoreResult<i64> {
        let profile = entry.profile_id.to_string();
        let mut tx = self.pool.begin().await?;
        let id = sqlx::query(
            "INSERT INTO query_history
                 (profile_id, sql, executed_at, duration_ms, status, rows_returned, rows_affected, error)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&profile)
        .bind(&entry.sql)
        .bind(entry.executed_at.to_rfc3339())
        .bind(i64::try_from(entry.duration.as_millis()).unwrap_or(i64::MAX))
        .bind(entry.status.as_str())
        .bind(entry.rows_returned.map(saturating_i64))
        .bind(entry.rows_affected.map(saturating_i64))
        .bind(&entry.error)
        .execute(&mut *tx)
        .await?
        .last_insert_rowid();
        sqlx::query(
            "DELETE FROM query_history WHERE profile_id = ?1 AND id <= (
                 SELECT id FROM query_history WHERE profile_id = ?1
                 ORDER BY id DESC LIMIT 1 OFFSET ?2)",
        )
        .bind(&profile)
        .bind(i64::try_from(HISTORY_RETENTION).unwrap_or(i64::MAX))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Matching entries, newest first. Unreadable rows are skipped.
    pub async fn list_history(&self, query: &HistoryQuery) -> StoreResult<Vec<HistoryEntry>> {
        let pattern = format!("%{}%", escape_like(&query.search));
        let rows = sqlx::query(
            "SELECT id, profile_id, sql, executed_at, duration_ms, status, rows_returned,
                    rows_affected, error
             FROM query_history
             WHERE (?1 IS NULL OR profile_id = ?1) AND sql LIKE ?2 ESCAPE '\\'
             ORDER BY id DESC LIMIT ?3",
        )
        .bind(query.profile_id.map(|p| p.to_string()))
        .bind(pattern)
        .bind(i64::try_from(query.limit).unwrap_or(i64::MAX))
        .fetch_all(&self.pool)
        .await?;
        let mut entries = Vec::with_capacity(rows.len());
        for row in rows {
            match decode(&row) {
                Ok(e) => entries.push(e),
                Err(e) => tracing::warn!(error = %e, "skipping unreadable history entry"),
            }
        }
        Ok(entries)
    }

    /// Deletes the history of one connection, or of every connection.
    pub async fn clear_history(&self, profile_id: Option<ProfileId>) -> StoreResult<()> {
        sqlx::query("DELETE FROM query_history WHERE ?1 IS NULL OR profile_id = ?1")
            .bind(profile_id.map(|p| p.to_string()))
            .execute(&self.pool)
            .await?;
        Ok(())
    }
}

fn decode(row: &SqliteRow) -> StoreResult<HistoryEntry> {
    let profile: String = row.try_get("profile_id")?;
    let executed_at: String = row.try_get("executed_at")?;
    let status: String = row.try_get("status")?;
    let duration_ms: i64 = row.try_get("duration_ms")?;
    let count = |name: &str| -> StoreResult<Option<u64>> {
        let v: Option<i64> = row.try_get(name)?;
        Ok(v.and_then(|n| u64::try_from(n).ok()))
    };
    Ok(HistoryEntry {
        id: row.try_get("id")?,
        profile_id: profile
            .parse()
            .map_err(|_| StoreError::Corrupt(format!("history profile id {profile}")))?,
        sql: row.try_get("sql")?,
        executed_at: DateTime::parse_from_rfc3339(&executed_at)
            .map_err(|_| StoreError::Corrupt(format!("history timestamp {executed_at}")))?
            .with_timezone(&Utc),
        duration: Duration::from_millis(u64::try_from(duration_ms).unwrap_or(0)),
        status: HistoryStatus::parse(&status)
            .ok_or_else(|| StoreError::Corrupt(format!("history status {status}")))?,
        rows_returned: count("rows_returned")?,
        rows_affected: count("rows_affected")?,
        error: row.try_get("error")?,
    })
}

fn saturating_i64(n: u64) -> i64 {
    i64::try_from(n).unwrap_or(i64::MAX)
}

/// Escapes `LIKE` wildcards so the search text matches literally.
fn escape_like(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if matches!(c, '%' | '_' | '\\') {
            out.push('\\');
        }
        out.push(c);
    }
    out
}

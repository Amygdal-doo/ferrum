//! The cookie jar, one row per cookie. The jar is small and saved whole.

use crate::{Store, StoreResult, now, to_json};
use ferrumapi_domain::{CookieJar, StoredCookie};
use sqlx::Row;

impl Store {
    /// Every stored cookie (expired ones included). Unreadable rows are skipped.
    pub async fn load_cookies(&self) -> StoreResult<CookieJar> {
        let rows = sqlx::query("SELECT data FROM cookies ORDER BY rowid")
            .fetch_all(&self.pool)
            .await?;
        let mut cookies = Vec::with_capacity(rows.len());
        for row in rows {
            let json: String = row.try_get("data")?;
            match serde_json::from_str::<StoredCookie>(&json) {
                Ok(c) => cookies.push(c),
                Err(e) => tracing::warn!(kind = ?e.classify(), "skipping unreadable cookie"),
            }
        }
        Ok(CookieJar::from_cookies(cookies))
    }

    /// Replaces the stored jar with `jar`.
    pub async fn save_cookies(&self, jar: &CookieJar) -> StoreResult<()> {
        let updated = now();
        let mut tx = self.pool.begin().await?;
        sqlx::query("DELETE FROM cookies").execute(&mut *tx).await?;
        for c in jar.cookies() {
            sqlx::query(
                "INSERT OR REPLACE INTO cookies (domain, path, name, data, updated_at)
                 VALUES (?, ?, ?, ?, ?)",
            )
            .bind(&c.domain)
            .bind(&c.path)
            .bind(&c.name)
            .bind(to_json(c)?)
            .bind(&updated)
            .execute(&mut *tx)
            .await?;
        }
        tx.commit().await?;
        tracing::debug!(count = jar.cookies().len(), "cookie jar saved");
        Ok(())
    }
}

#[cfg(test)]
#[path = "cookies_tests.rs"]
mod tests;

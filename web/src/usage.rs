//! Anonymous usage counts sent by FerrumLauncher once a day.
//!
//! [`parse_ping`] validates a `POST /api/ping` body (pure, unit-tested).
//! [`Usage`] stores one row per install per UTC day in Postgres and serves
//! cached aggregates for `/stats`. Without `DATABASE_URL` it stores nothing
//! and `/stats` shows an empty state. No IP addresses or user agents are
//! ever stored. What is sent: `/privacy#usage-counts`.

use std::{
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use serde::Deserialize;
use sqlx::postgres::{PgPool, PgPoolOptions};
use uuid::Uuid;

use crate::products;

/// Largest accepted request body.
pub const MAX_BODY_BYTES: usize = 8 * 1024;
/// [`parse_ping`]'s error for a body over [`MAX_BODY_BYTES`].
pub const TOO_LARGE: &str = "body too large";
/// Most apps one ping may list.
pub const MAX_APPS: usize = 64;
const MAX_FIELD_LEN: usize = 32;
pub const OS_NAMES: [&str; 3] = ["macos", "linux", "windows"];
pub const ARCH_NAMES: [&str; 2] = ["aarch64", "x86_64"];
/// How long `/stats` reuses its aggregates.
pub const CACHE_TTL: Duration = Duration::from_secs(5 * 60);

/// The body the launcher sends. Unknown fields are ignored (and not stored).
#[derive(Debug, Deserialize)]
struct RawPing {
    install_id: String,
    launcher_version: String,
    os: String,
    arch: String,
    #[serde(default)]
    apps: Vec<RawApp>,
}

#[derive(Debug, Deserialize)]
struct RawApp {
    id: String,
    version: String,
}

/// A validated ping.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ping {
    pub install_id: Uuid,
    pub launcher_version: String,
    pub os: &'static str,
    pub arch: &'static str,
    /// Known app ids only, each once: `(id, version)`.
    pub apps: Vec<(&'static str, String)>,
}

/// A version string: 1–32 of `0-9 A-Z a-z . + -`.
fn valid_version(v: &str) -> bool {
    !v.is_empty()
        && v.len() <= MAX_FIELD_LEN
        && v.chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '+' | '-'))
}

fn known<const N: usize>(names: [&'static str; N], value: &str) -> Option<&'static str> {
    names.into_iter().find(|n| *n == value)
}

/// Validates a ping body. Apps with unknown ids are dropped; anything else
/// out of shape rejects the whole ping.
pub fn parse_ping(body: &[u8]) -> Result<Ping, &'static str> {
    if body.len() > MAX_BODY_BYTES {
        return Err(TOO_LARGE);
    }
    let raw: RawPing = serde_json::from_slice(body).map_err(|_| "invalid JSON")?;
    let install_id = Uuid::parse_str(&raw.install_id).map_err(|_| "invalid install_id")?;
    if install_id.is_nil() || raw.install_id.len() != 36 {
        return Err("invalid install_id");
    }
    if !valid_version(&raw.launcher_version) {
        return Err("invalid launcher_version");
    }
    let os = known(OS_NAMES, &raw.os).ok_or("unknown os")?;
    let arch = known(ARCH_NAMES, &raw.arch).ok_or("unknown arch")?;
    if raw.apps.len() > MAX_APPS {
        return Err("too many apps");
    }
    let mut apps: Vec<(&'static str, String)> = Vec::new();
    for app in raw.apps {
        if !valid_version(&app.version) || app.id.len() > MAX_FIELD_LEN {
            return Err("invalid app");
        }
        let Some(product) = products::apps().find(|p| p.logo == app.id) else {
            continue;
        };
        if !apps.iter().any(|(id, _)| *id == product.logo) {
            apps.push((product.logo, app.version));
        }
    }
    Ok(Ping {
        install_id,
        launcher_version: raw.launcher_version,
        os,
        arch,
        apps,
    })
}

/// What `/stats` shows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aggregates {
    /// Installs that sent a ping today (UTC), in the last 7 and 30 days.
    pub today: i64,
    pub last_7_days: i64,
    pub last_30_days: i64,
    /// `(os, arch, installs)` over 30 days, most first.
    pub platforms: Vec<(String, String, i64)>,
    /// `(app id, installs)` over 30 days, most first.
    pub apps: Vec<(String, i64)>,
}

/// The ping store. Cheap to share behind an `Arc`.
pub struct Usage {
    pool: Option<PgPool>,
    cache: Mutex<Option<(Instant, Arc<Aggregates>)>>,
}

impl std::fmt::Debug for Usage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Usage")
            .field("database", &self.pool.is_some())
            .finish_non_exhaustive()
    }
}

impl Usage {
    /// A lazily connecting pool for `database_url`, if any. A malformed URL
    /// is logged and treated as no database.
    pub fn from_url(database_url: Option<&str>) -> Self {
        let pool = database_url.and_then(|url| {
            PgPoolOptions::new()
                .max_connections(5)
                .acquire_timeout(Duration::from_secs(5))
                .connect_lazy(url)
                .inspect_err(|err| tracing::error!(%err, "DATABASE_URL is not usable"))
                .ok()
        });
        Self {
            pool,
            cache: Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        self.pool.is_some()
    }

    /// The database, shared with the page-view counter (`crate::visits`).
    pub fn pool(&self) -> Option<&PgPool> {
        self.pool.as_ref()
    }

    /// Applies `migrations/` (embedded at build time). No-op without a database.
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        match &self.pool {
            Some(pool) => sqlx::migrate!("./migrations").run(pool).await,
            None => Ok(()),
        }
    }

    /// Stores `ping` for today (UTC). A second ping from the same install on
    /// the same day changes nothing. `Ok(false)` without a database.
    pub async fn record(&self, ping: &Ping) -> Result<bool, sqlx::Error> {
        let Some(pool) = &self.pool else {
            return Ok(false);
        };
        let (ids, versions): (Vec<&str>, Vec<&str>) = ping
            .apps
            .iter()
            .map(|(id, version)| (*id, version.as_str()))
            .unzip();
        // The day is the database's UTC date, not the client's.
        sqlx::query(
            "WITH ping AS (
                 INSERT INTO usage_pings (day, install_id, launcher_version, os, arch)
                 VALUES ((now() AT TIME ZONE 'UTC')::date, $1, $2, $3, $4)
                 ON CONFLICT (day, install_id) DO NOTHING
                 RETURNING day, install_id
             )
             INSERT INTO usage_ping_apps (day, install_id, app_id, version)
             SELECT ping.day, ping.install_id, a.id, a.version
             FROM ping, UNNEST($5::text[], $6::text[]) AS a(id, version)
             ON CONFLICT DO NOTHING",
        )
        .bind(ping.install_id)
        .bind(&ping.launcher_version)
        .bind(ping.os)
        .bind(ping.arch)
        .bind(&ids)
        .bind(&versions)
        .execute(pool)
        .await?;
        Ok(true)
    }

    /// Aggregates for `/stats`, cached for [`CACHE_TTL`]. `None` without a
    /// database or when it can't be read (and nothing is cached).
    pub async fn aggregates(&self) -> Option<Arc<Aggregates>> {
        let pool = self.pool.as_ref()?;
        let stale = {
            let cache = self.cache.lock().unwrap_or_else(|e| e.into_inner());
            match cache.as_ref() {
                Some((at, a)) if at.elapsed() < CACHE_TTL => return Some(a.clone()),
                Some((_, a)) => Some(a.clone()),
                None => None,
            }
        };
        match query_aggregates(pool).await {
            Ok(fresh) => {
                let fresh = Arc::new(fresh);
                *self.cache.lock().unwrap_or_else(|e| e.into_inner()) =
                    Some((Instant::now(), fresh.clone()));
                Some(fresh)
            }
            Err(err) => {
                tracing::warn!(%err, "could not read usage counts");
                stale
            }
        }
    }
}

async fn query_aggregates(pool: &PgPool) -> Result<Aggregates, sqlx::Error> {
    let counts: (i64, i64, i64) = sqlx::query_as(
        "WITH t AS (SELECT (now() AT TIME ZONE 'UTC')::date AS today)
         SELECT
             count(DISTINCT install_id) FILTER (WHERE day = t.today),
             count(DISTINCT install_id) FILTER (WHERE day > t.today - 7),
             count(DISTINCT install_id)
         FROM usage_pings, t WHERE day > t.today - 30",
    )
    .fetch_one(pool)
    .await?;
    let platforms = sqlx::query_as(
        "SELECT os, arch, count(DISTINCT install_id) AS n
         FROM usage_pings WHERE day > (now() AT TIME ZONE 'UTC')::date - 30
         GROUP BY os, arch ORDER BY n DESC, os, arch",
    )
    .fetch_all(pool)
    .await?;
    let apps = sqlx::query_as(
        "SELECT app_id, count(DISTINCT install_id) AS n
         FROM usage_ping_apps WHERE day > (now() AT TIME ZONE 'UTC')::date - 30
         GROUP BY app_id ORDER BY n DESC, app_id",
    )
    .fetch_all(pool)
    .await?;
    Ok(Aggregates {
        today: counts.0,
        last_7_days: counts.1,
        last_30_days: counts.2,
        platforms,
        apps,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;

    const ID: &str = "0b6f1f8e-6c5a-4d43-9a1e-2f1d8e4b7c10";

    fn body(apps: &str) -> String {
        format!(
            r#"{{"install_id":"{ID}","launcher_version":"0.9.1","os":"macos","arch":"aarch64","apps":{apps}}}"#
        )
    }

    #[test]
    fn accepts_a_ping_and_drops_unknown_apps() {
        let ping = parse_ping(
            body(r#"[{"id":"git","version":"0.4.0"},{"id":"nope","version":"1.0.0"},{"id":"git","version":"9.9.9"}]"#)
                .as_bytes(),
        )
        .unwrap();
        assert_eq!(ping.install_id.to_string(), ID);
        assert_eq!(ping.os, "macos");
        assert_eq!(ping.arch, "aarch64");
        assert_eq!(ping.apps, vec![("git", "0.4.0".to_owned())]);
        // The launcher is reported as `launcher_version`, not as an app.
        let ping = parse_ping(body(r#"[{"id":"launcher","version":"1.0.0"}]"#).as_bytes()).unwrap();
        assert!(ping.apps.is_empty());
        // Extra fields are ignored, not stored.
        let extra = body("[]").replace('}', r#","hostname":"box"}"#);
        assert!(parse_ping(extra.as_bytes()).is_ok());
    }

    #[test]
    fn rejects_bad_pings() {
        let cases = [
            ("not json".to_owned(), "invalid JSON"),
            (body("[]").replace(ID, "abc"), "invalid install_id"),
            (
                body("[]").replace(ID, "00000000-0000-0000-0000-000000000000"),
                "invalid install_id",
            ),
            (body("[]").replace("macos", "beos"), "unknown os"),
            (body("[]").replace("aarch64", "mips"), "unknown arch"),
            (
                body("[]").replace("0.9.1", &"9".repeat(33)),
                "invalid launcher_version",
            ),
            (
                body("[]").replace("0.9.1", "1.0 <script>"),
                "invalid launcher_version",
            ),
            (body(r#"[{"id":"git","version":""}]"#), "invalid app"),
            (body(r#"[{"id":"git"}]"#), "invalid JSON"),
        ];
        for (input, error) in cases {
            assert_eq!(parse_ping(input.as_bytes()), Err(error), "{input}");
        }
        let many = format!(
            "[{}]",
            vec![r#"{"id":"git","version":"1.0.0"}"#; MAX_APPS + 1].join(",")
        );
        assert_eq!(parse_ping(body(&many).as_bytes()), Err("too many apps"));
        let big = body("[]").replace("macos", &"x".repeat(MAX_BODY_BYTES));
        assert_eq!(parse_ping(big.as_bytes()), Err("body too large"));
    }

    #[tokio::test]
    async fn without_a_database_nothing_is_stored() {
        let usage = Usage::from_url(None);
        assert!(!usage.enabled());
        let ping = parse_ping(body("[]").as_bytes()).unwrap();
        assert!(!usage.record(&ping).await.unwrap());
        assert!(usage.aggregates().await.is_none());
        usage.migrate().await.unwrap();
    }
}

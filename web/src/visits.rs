//! Page views counted on the server, without cookies.
//!
//! [`countable`] and [`source`] decide what a request is (pure, unit-tested).
//! [`Visits`] adds views up in memory and [`Visits::flush`] writes the totals
//! per UTC day, page and source to Postgres, for `/stats`. Only the page path
//! and a site name taken from the `Referer` header are kept: no IP address,
//! cookie or user agent (read only to skip bots) is ever stored. Without a
//! database nothing is kept. What is counted: `/privacy#page-views`.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use sqlx::postgres::PgPool;

/// How often [`Visits::flush`] should run.
pub const FLUSH_EVERY: Duration = Duration::from_secs(60);
/// How long `/stats` reuses its totals.
pub const CACHE_TTL: Duration = Duration::from_secs(5 * 60);
/// Distinct `(page, source)` pairs held between flushes; more are dropped.
const MAX_PENDING: usize = 1_000;
const MAX_PATH_LEN: usize = 128;

/// Where a view came from: a key of [`SOURCES`], `internal` (a click on this
/// site), `direct` (no `Referer`) or `other`.
pub const INTERNAL: &str = "internal";
const DIRECT: &str = "direct";
const OTHER: &str = "other";

/// Known referring sites: `(key, hosts)`. A host matches itself and its
/// subdomains.
const SOURCES: [(&str, &[&str]); 8] = [
    ("x", &["x.com", "twitter.com", "t.co"]),
    ("linkedin", &["linkedin.com", "lnkd.in"]),
    ("hackernews", &["news.ycombinator.com"]),
    ("reddit", &["reddit.com"]),
    ("github", &["github.com"]),
    ("bing", &["bing.com"]),
    ("duckduckgo", &["duckduckgo.com"]),
    ("google", &["google.com"]),
];

/// The display name of a source key.
pub fn source_label(key: &str) -> &str {
    match key {
        "x" => "X (Twitter)",
        "linkedin" => "LinkedIn",
        "hackernews" => "Hacker News",
        "reddit" => "Reddit",
        "github" => "GitHub",
        "bing" => "Bing",
        "duckduckgo" => "DuckDuckGo",
        "google" => "Google",
        DIRECT => "Direct or unknown",
        OTHER => "Other sites",
        other => other,
    }
}

fn host_of(url: &str) -> Option<String> {
    let rest = url.split_once("://")?.1;
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit('@').next()?.split(':').next()?;
    (!host.is_empty()).then(|| host.to_ascii_lowercase())
}

fn matches(host: &str, domain: &str) -> bool {
    host == domain
        || host
            .strip_suffix(domain)
            .is_some_and(|sub| sub.ends_with('.'))
}

/// The source of a view with `referer`, on a site served from `site_host`.
pub fn source(referer: Option<&str>, site_host: &str) -> &'static str {
    let Some(host) = referer.and_then(host_of) else {
        return DIRECT;
    };
    if matches(&host, site_host) {
        return INTERNAL;
    }
    // Google has a domain per country (google.de, google.co.uk, …).
    if host.split('.').any(|label| label == "google") {
        return "google";
    }
    SOURCES
        .iter()
        .find(|(_, hosts)| hosts.iter().any(|d| matches(&host, d)))
        .map_or(OTHER, |(key, _)| key)
}

/// Whether a user agent looks like a person's browser rather than a bot,
/// a link preview or a script.
pub fn is_browser(user_agent: &str) -> bool {
    const NOT_A_PERSON: [&str; 16] = [
        "bot",
        "crawl",
        "spider",
        "slurp",
        "preview",
        "facebookexternalhit",
        "embedly",
        "headless",
        "lighthouse",
        "monitor",
        "uptime",
        "curl",
        "wget",
        "python",
        "http",
        "fetch",
    ];
    let ua = user_agent.to_ascii_lowercase();
    ua.starts_with("mozilla/") && !NOT_A_PERSON.iter().any(|t| ua.contains(t))
}

/// Whether a `GET` for `path` is a page view worth counting. Assets, the API,
/// health checks and the proxied account pages are not.
pub fn countable(path: &str) -> bool {
    const SKIP: [&str; 9] = [
        "/assets/", "/api/", "/healthz", "/login", "/signup", "/logout", "/account", "/auth/",
        "/oauth/",
    ];
    path.starts_with('/') && path.len() <= MAX_PATH_LEN && !SKIP.iter().any(|p| path.starts_with(p))
}

/// What `/stats` shows. Visits are views that came from outside the site.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Aggregates {
    pub today: i64,
    pub last_7_days: i64,
    pub last_30_days: i64,
    /// `(source key, visits)` over 30 days, most first.
    pub sources: Vec<(String, i64)>,
    /// `(path, views)` over 30 days, most first, at most 10.
    pub pages: Vec<(String, i64)>,
}

/// The page-view counter. Cheap to share behind an `Arc`.
pub struct Visits {
    pool: Option<PgPool>,
    site_host: String,
    pending: Mutex<HashMap<(String, &'static str), i64>>,
    cache: Mutex<Option<(Instant, Arc<Aggregates>)>>,
}

impl std::fmt::Debug for Visits {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Visits")
            .field("database", &self.pool.is_some())
            .finish_non_exhaustive()
    }
}

impl Visits {
    /// Counts into `pool` (usually the usage database) for the site at
    /// `site_url`. Without a pool, views are not kept.
    pub fn new(pool: Option<PgPool>, site_url: &str) -> Self {
        Self {
            pool,
            site_host: host_of(site_url).unwrap_or_default(),
            pending: Mutex::new(HashMap::new()),
            cache: Mutex::new(None),
        }
    }

    pub fn enabled(&self) -> bool {
        self.pool.is_some()
    }

    pub fn site_host(&self) -> &str {
        &self.site_host
    }

    /// Adds one view of `path` from `source` to the next flush.
    pub fn add(&self, path: &str, source: &'static str) {
        if !self.enabled() {
            return;
        }
        let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
        add_to(&mut pending, (path.to_owned(), source), 1);
    }

    /// Writes the views added since the last flush. If the database can't be
    /// written they are kept for the next try.
    pub async fn flush(&self) {
        let Some(pool) = &self.pool else { return };
        let batch = std::mem::take(&mut *self.pending.lock().unwrap_or_else(|e| e.into_inner()));
        if batch.is_empty() {
            return;
        }
        let mut paths = Vec::with_capacity(batch.len());
        let mut sources = Vec::with_capacity(batch.len());
        let mut views = Vec::with_capacity(batch.len());
        for ((path, source), n) in &batch {
            paths.push(path.as_str());
            sources.push(*source);
            views.push(*n);
        }
        let written = sqlx::query(
            "INSERT INTO site_views (day, path, source, views)
             SELECT (now() AT TIME ZONE 'UTC')::date, v.path, v.source, v.views
             FROM UNNEST($1::text[], $2::text[], $3::int8[]) AS v(path, source, views)
             ON CONFLICT (day, path, source) DO UPDATE
             SET views = site_views.views + EXCLUDED.views",
        )
        .bind(&paths)
        .bind(&sources)
        .bind(&views)
        .execute(pool)
        .await;
        if let Err(err) = written {
            tracing::warn!(%err, "could not store page views");
            let mut pending = self.pending.lock().unwrap_or_else(|e| e.into_inner());
            for (key, n) in batch {
                add_to(&mut pending, key, n);
            }
        }
    }

    /// Totals for `/stats`, cached for [`CACHE_TTL`]. `None` without a
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
                tracing::warn!(%err, "could not read page views");
                stale
            }
        }
    }

    #[cfg(test)]
    fn pending(&self) -> HashMap<(String, &'static str), i64> {
        self.pending
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }
}

fn add_to(pending: &mut HashMap<(String, &'static str), i64>, key: (String, &'static str), n: i64) {
    let full = pending.len() >= MAX_PENDING;
    match pending.get_mut(&key) {
        Some(views) => *views += n,
        None if !full => {
            pending.insert(key, n);
        }
        None => {}
    }
}

async fn query_aggregates(pool: &PgPool) -> Result<Aggregates, sqlx::Error> {
    let counts: (i64, i64, i64) = sqlx::query_as(
        "WITH t AS (SELECT (now() AT TIME ZONE 'UTC')::date AS today)
         SELECT
             COALESCE(sum(views) FILTER (WHERE day = t.today), 0)::int8,
             COALESCE(sum(views) FILTER (WHERE day > t.today - 7), 0)::int8,
             COALESCE(sum(views), 0)::int8
         FROM site_views, t WHERE day > t.today - 30 AND source <> 'internal'",
    )
    .fetch_one(pool)
    .await?;
    let sources = sqlx::query_as(
        "SELECT source, sum(views)::int8 AS n
         FROM site_views
         WHERE day > (now() AT TIME ZONE 'UTC')::date - 30 AND source <> 'internal'
         GROUP BY source ORDER BY n DESC, source",
    )
    .fetch_all(pool)
    .await?;
    let pages = sqlx::query_as(
        "SELECT path, sum(views)::int8 AS n
         FROM site_views WHERE day > (now() AT TIME ZONE 'UTC')::date - 30
         GROUP BY path ORDER BY n DESC, path LIMIT 10",
    )
    .fetch_all(pool)
    .await?;
    Ok(Aggregates {
        today: counts.0,
        last_7_days: counts.1,
        last_30_days: counts.2,
        sources,
        pages,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SITE: &str = "tryferrum.dev";

    #[test]
    fn names_where_a_view_came_from() {
        for (referer, expected) in [
            (None, "direct"),
            (Some("not a url"), "direct"),
            (Some("https://tryferrum.dev/apps"), "internal"),
            (Some("https://www.tryferrum.dev/"), "internal"),
            (Some("https://t.co/"), "x"),
            (Some("https://x.com/someone/status/1"), "x"),
            (Some("https://www.linkedin.com/"), "linkedin"),
            (Some("https://lnkd.in/abc"), "linkedin"),
            (Some("https://news.ycombinator.com/item?id=1"), "hackernews"),
            (Some("https://old.reddit.com/r/rust/"), "reddit"),
            (Some("https://github.com/Amygdal-doo/ferrum"), "github"),
            (Some("https://www.google.co.uk/"), "google"),
            (Some("https://duckduckgo.com/"), "duckduckgo"),
            (Some("https://notlinkedin.com/"), "other"),
            (Some("https://tryferrum.dev.evil.example/"), "other"),
            (Some("https://user@linkedin.com:443/"), "linkedin"),
        ] {
            assert_eq!(source(referer, SITE), expected, "{referer:?}");
        }
        assert_eq!(source_label("x"), "X (Twitter)");
        assert_eq!(source_label("direct"), "Direct or unknown");
    }

    #[test]
    fn counts_browsers_but_not_bots_or_scripts() {
        assert!(is_browser(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/18.0 Safari/605.1.15"
        ));
        for ua in [
            "",
            "curl/8.7.1",
            "Twitterbot/1.0",
            "LinkedInBot/1.0 (compatible; Mozilla/5.0; Apache-HttpClient +http://www.linkedin.com)",
            "Mozilla/5.0 (compatible; Googlebot/2.1; +http://www.google.com/bot.html)",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 HeadlessChrome/120.0",
            "facebookexternalhit/1.1",
            "python-requests/2.32",
        ] {
            assert!(!is_browser(ua), "{ua}");
        }
    }

    #[test]
    fn counts_pages_only() {
        for path in ["/", "/apps", "/updates/some-post", "/stats"] {
            assert!(countable(path), "{path}");
        }
        let long = format!("/{}", "a".repeat(MAX_PATH_LEN));
        for path in [
            "/assets/css/site.css",
            "/api/ping",
            "/healthz",
            "/login",
            "/account/x",
            "",
        ] {
            assert!(!countable(path), "{path}");
        }
        assert!(!countable(&long));
    }

    #[tokio::test]
    async fn without_a_database_nothing_is_kept() {
        let visits = Visits::new(None, "https://tryferrum.dev");
        assert!(!visits.enabled());
        assert_eq!(visits.site_host(), SITE);
        visits.add("/", "x");
        assert!(visits.pending().is_empty());
        visits.flush().await;
        assert!(visits.aggregates().await.is_none());
    }

    #[test]
    fn pending_views_add_up_and_stay_bounded() {
        let mut pending = HashMap::new();
        add_to(&mut pending, ("/".into(), "x"), 1);
        add_to(&mut pending, ("/".into(), "x"), 2);
        assert_eq!(pending[&("/".to_owned(), "x")], 3);
        for i in 0..MAX_PENDING * 2 {
            add_to(&mut pending, (format!("/{i}"), "direct"), 1);
        }
        assert_eq!(pending.len(), MAX_PENDING);
        // Known keys still count when full.
        add_to(&mut pending, ("/".into(), "x"), 1);
        assert_eq!(pending[&("/".to_owned(), "x")], 4);
    }
}

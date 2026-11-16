//! Runtime configuration read from the environment.
//!
//! Parsing lives in [`Config::from_vars`], a pure function over a lookup
//! closure, so it can be unit-tested without mutating the process env.

use std::net::{Ipv4Addr, SocketAddr};

/// Default port when `PORT` is unset (Railway always sets it).
pub const DEFAULT_PORT: u16 = 8080;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub port: u16,
    /// Public base URL without trailing slash, used for canonical/OG/sitemap URLs.
    pub site_url: String,
    /// Postgres for the anonymous usage counts (`POST /api/ping`, `/stats`).
    /// `None` stores nothing (local dev, tests).
    pub database_url: Option<String>,
}

/// Look up a variable, treating empty / whitespace-only values as unset.
fn non_empty(get: &impl Fn(&str) -> Option<String>, key: &str) -> Option<String> {
    get(key)
        .map(|v| v.trim().to_owned())
        .filter(|v| !v.is_empty())
}

impl Config {
    /// Read configuration from the process environment.
    pub fn from_env() -> Self {
        Self::from_vars(|key| std::env::var(key).ok())
    }

    /// Build configuration from an arbitrary variable lookup.
    ///
    /// - `PORT`: u16, default [`DEFAULT_PORT`]; invalid values fall back to it.
    /// - `SITE_URL`: public base URL; else `https://$RAILWAY_PUBLIC_DOMAIN`;
    ///   else `http://localhost:$PORT`. A trailing `/` is stripped.
    /// - `DATABASE_URL`: Postgres connection string; unset or empty means none.
    pub fn from_vars(get: impl Fn(&str) -> Option<String>) -> Self {
        let port = non_empty(&get, "PORT")
            .and_then(|p| p.parse().ok())
            .unwrap_or(DEFAULT_PORT);
        let site_url = non_empty(&get, "SITE_URL")
            .or_else(|| non_empty(&get, "RAILWAY_PUBLIC_DOMAIN").map(|d| format!("https://{d}")))
            .unwrap_or_else(|| format!("http://localhost:{port}"));
        Self {
            port,
            site_url: site_url.trim_end_matches('/').to_owned(),
            database_url: non_empty(&get, "DATABASE_URL"),
        }
    }

    pub fn bind_addr(&self) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::UNSPECIFIED, self.port))
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_vars(|_| None)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(vars: &[(&str, &str)]) -> Config {
        Config::from_vars(|key| {
            vars.iter()
                .find(|(k, _)| *k == key)
                .map(|(_, v)| (*v).to_owned())
        })
    }

    #[test]
    fn defaults_when_nothing_set() {
        let c = cfg(&[]);
        assert_eq!(c.port, 8080);
        assert_eq!(c.site_url, "http://localhost:8080");
        assert_eq!(c.database_url, None);
        assert_eq!(c, Config::default());
    }

    #[test]
    fn reads_database_url() {
        let c = cfg(&[("DATABASE_URL", " postgres://u:p@db.internal:5432/railway ")]);
        assert_eq!(
            c.database_url.as_deref(),
            Some("postgres://u:p@db.internal:5432/railway")
        );
        assert_eq!(cfg(&[("DATABASE_URL", "")]).database_url, None);
    }

    #[test]
    fn parses_port_and_uses_it_for_local_url() {
        let c = cfg(&[("PORT", "3000")]);
        assert_eq!(c.port, 3000);
        assert_eq!(c.site_url, "http://localhost:3000");
        assert_eq!(c.bind_addr().to_string(), "0.0.0.0:3000");
    }

    #[test]
    fn invalid_or_empty_port_falls_back() {
        assert_eq!(cfg(&[("PORT", "abc")]).port, DEFAULT_PORT);
        assert_eq!(cfg(&[("PORT", "70000")]).port, DEFAULT_PORT);
        assert_eq!(cfg(&[("PORT", "")]).port, DEFAULT_PORT);
    }

    #[test]
    fn site_url_wins_and_trailing_slash_is_stripped() {
        let c = cfg(&[
            ("SITE_URL", "https://ferrum.dev/"),
            ("RAILWAY_PUBLIC_DOMAIN", "x.up.railway.app"),
        ]);
        assert_eq!(c.site_url, "https://ferrum.dev");
    }

    #[test]
    fn falls_back_to_railway_domain() {
        let c = cfg(&[
            ("SITE_URL", " "),
            ("RAILWAY_PUBLIC_DOMAIN", "x.up.railway.app"),
        ]);
        assert_eq!(c.site_url, "https://x.up.railway.app");
    }
}

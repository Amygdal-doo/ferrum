//! Connection and parsing helpers shared by commands.

use ferrumterm_client::{Client, Endpoint, connect_or_start, paths};
use ferrumterm_protocol::Event;
use std::time::Duration;
use tokio::sync::mpsc::UnboundedReceiver;

pub struct Ctx {
    pub server: Option<String>,
    pub token: Option<String>,
    pub json: bool,
}

impl Ctx {
    /// Remote when `-S` names a saved host or an endpoint; else the local server (started on demand).
    pub fn is_local(&self) -> bool {
        self.server.is_none()
    }

    pub async fn connect(&self) -> Result<(Client, UnboundedReceiver<Event>), String> {
        match &self.server {
            None => connect_or_start(&paths::socket(), "fterm")
                .await
                .map_err(|e| e.to_string()),
            Some(s) => {
                let endpoint = match crate::hosts::lookup(s) {
                    Some(h) => Endpoint::parse(&h.endpoint, self.token.clone().or(h.token)),
                    None => Endpoint::parse(s, self.token.clone()),
                };
                Client::connect(&endpoint, "fterm")
                    .await
                    .map_err(|e| e.to_string())
            }
        }
    }

    /// Explicit target, else the caller's own block when running inside FerrumTerm (local server only).
    pub fn target(&self, explicit: Option<String>) -> Option<String> {
        explicit.or_else(|| {
            self.is_local()
                .then(|| std::env::var("FTERM_BLOCK").ok())
                .flatten()
        })
    }
}

/// `90`, `90s`, `5m`, `2h`, `250ms`.
pub fn parse_duration(s: &str) -> Result<Duration, String> {
    let s = s.trim();
    let (num, unit) = s
        .find(|c: char| !c.is_ascii_digit() && c != '.')
        .map_or((s, "s"), |i| s.split_at(i));
    let n: f64 = num.parse().map_err(|_| format!("invalid duration: {s}"))?;
    let secs = match unit {
        "ms" => n / 1000.0,
        "s" | "" => n,
        "m" => n * 60.0,
        "h" => n * 3600.0,
        _ => return Err(format!("invalid duration unit in {s} (use ms, s, m, h)")),
    };
    Ok(Duration::from_secs_f64(secs))
}

/// The caller's environment variables worth passing to new programs (so
/// commands like `claude` resolve with your PATH even when the server was
/// started by launchd).
pub fn passthrough_env() -> Vec<(String, String)> {
    ["PATH", "LANG", "LC_ALL", "SSH_AUTH_SOCK"]
        .iter()
        .filter_map(|k| std::env::var(k).ok().map(|v| ((*k).to_owned(), v)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse_duration;
    use std::time::Duration;

    #[test]
    fn durations() {
        assert_eq!(parse_duration("90").ok(), Some(Duration::from_secs(90)));
        assert_eq!(parse_duration("5m").ok(), Some(Duration::from_secs(300)));
        assert_eq!(
            parse_duration("250ms").ok(),
            Some(Duration::from_millis(250))
        );
        assert_eq!(parse_duration("1.5h").ok(), Some(Duration::from_secs(5400)));
        assert!(parse_duration("10y").is_err());
    }
}

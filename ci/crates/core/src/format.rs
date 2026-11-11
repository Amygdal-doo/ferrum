//! Durations and relative times as the UI shows them.

use chrono::{DateTime, Utc};

/// `42s`, `3m 05s`, `1h 02m`.
#[must_use]
pub fn duration(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        s if s < 60 => format!("{s}s"),
        s if s < 3600 => format!("{}m {:02}s", s / 60, s % 60),
        s => format!("{}h {:02}m", s / 3600, (s % 3600) / 60),
    }
}

/// `just now`, `5m ago`, `3h ago`, `2d ago`, then the date.
#[must_use]
pub fn relative(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let secs = (now - then).num_seconds();
    match secs {
        s if s < 45 => "just now".to_owned(),
        s if s < 3600 => format!("{}m ago", (s + 30) / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s if s < 30 * 86_400 => format!("{}d ago", s / 86_400),
        _ => then.format("%Y-%m-%d").to_string(),
    }
}

/// `abc1234` from a full SHA.
#[must_use]
pub fn short_sha(sha: &str) -> &str {
    sha.get(..7).unwrap_or(sha)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations() {
        assert_eq!(duration(-3), "0s");
        assert_eq!(duration(42), "42s");
        assert_eq!(duration(185), "3m 05s");
        assert_eq!(duration(3720), "1h 02m");
    }

    #[test]
    fn relative_times() {
        let now: DateTime<Utc> = "2024-05-10T12:00:00Z".parse().expect("now");
        let ago = |s: i64| relative(now - chrono::Duration::seconds(s), now);
        assert_eq!(ago(10), "just now");
        assert_eq!(ago(300), "5m ago");
        assert_eq!(ago(7200), "2h ago");
        assert_eq!(ago(3 * 86_400), "3d ago");
        assert_eq!(ago(40 * 86_400), "2024-03-31");
        assert_eq!(short_sha("0123456789"), "0123456");
        assert_eq!(short_sha("01"), "01");
    }
}

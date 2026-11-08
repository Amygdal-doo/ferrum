//! Display helpers.

use chrono::{DateTime, Local, TimeZone};

/// "just now", "5 min ago", "3 h ago", "2 days ago", else a date.
#[must_use]
pub fn relative_time(unix_secs: i64, now_secs: i64) -> String {
    let d = now_secs - unix_secs;
    match d {
        i64::MIN..60 => "just now".into(),
        60..3_600 => format!("{} min ago", d / 60),
        3_600..86_400 => format!("{} h ago", d / 3_600),
        86_400..172_800 => "yesterday".into(),
        172_800..2_592_000 => format!("{} days ago", d / 86_400),
        _ => local(unix_secs).map_or_else(String::new, |t| t.format("%Y-%m-%d").to_string()),
    }
}

/// Full local timestamp for the commit details.
#[must_use]
pub fn full_time(unix_secs: i64) -> String {
    local(unix_secs).map_or_else(String::new, |t| {
        t.format("%Y-%m-%d %H:%M:%S %z").to_string()
    })
}

fn local(unix_secs: i64) -> Option<DateTime<Local>> {
    Local.timestamp_opt(unix_secs, 0).single()
}

/// First letters of up to two words, for author avatars.
#[must_use]
pub fn initials(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|w| w.chars().next())
        .take(2)
        .flat_map(char::to_uppercase)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn relative_buckets() {
        assert_eq!(relative_time(1000, 1010), "just now");
        assert_eq!(relative_time(0, 300), "5 min ago");
        assert_eq!(relative_time(0, 7_200), "2 h ago");
        assert_eq!(relative_time(0, 100_000), "yesterday");
        assert_eq!(relative_time(0, 3 * 86_400), "3 days ago");
    }

    #[test]
    fn initials_of_names() {
        assert_eq!(initials("Ada Lovelace"), "AL");
        assert_eq!(initials("linus"), "L");
        assert_eq!(initials(""), "");
    }
}

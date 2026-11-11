//! Parser for `ps -ww -o pid=,user=,lstart=,command= -p <pids>` (run with
//! `LC_ALL=C` so `lstart` is `Sun Nov  1 22:37:39 2026` everywhere).

use std::collections::HashMap;

use chrono::NaiveDateTime;

/// What `ps` knows about one process.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PsEntry {
    pub user: String,
    pub started: Option<NaiveDateTime>,
    pub command: String,
}

/// `ps` arguments before the comma-separated pid list.
pub const ARGS: &[&str] = &["-ww", "-o", "pid=,user=,lstart=,command=", "-p"];

/// The first whitespace-separated token and the rest of `text`.
fn token(text: &str) -> Option<(&str, &str)> {
    let text = text.trim_start();
    if text.is_empty() {
        return None;
    }
    Some(text.split_once(char::is_whitespace).unwrap_or((text, "")))
}

#[must_use]
pub fn parse(text: &str) -> HashMap<u32, PsEntry> {
    text.lines().filter_map(parse_line).collect()
}

fn parse_line(line: &str) -> Option<(u32, PsEntry)> {
    let (pid, rest) = token(line)?;
    let pid = pid.parse().ok()?;
    let (user, mut rest) = token(rest)?;
    let mut date = Vec::with_capacity(5);
    for _ in 0..5 {
        let (part, tail) = token(rest)?;
        date.push(part);
        rest = tail;
    }
    let started = NaiveDateTime::parse_from_str(&date.join(" "), "%a %b %d %H:%M:%S %Y").ok();
    Some((
        pid,
        PsEntry {
            user: user.to_string(),
            started,
            command: rest.trim().to_string(),
        },
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Datelike, Timelike};

    #[test]
    fn parses_ps_lines() {
        let text = "  626 abdul    Thu Oct 29 16:09:55 2026     /usr/libexec/rapportd\n 1053 root     Sun Nov  1 22:37:39 2026     adb -L tcp:5037 fork-server server --reply-fd 4\n";
        let entries = parse(text);
        assert_eq!(entries.len(), 2);
        let adb = &entries[&1053];
        assert_eq!(adb.user, "root");
        assert_eq!(
            adb.command,
            "adb -L tcp:5037 fork-server server --reply-fd 4"
        );
        let started = adb.started.expect("date");
        assert_eq!((started.day(), started.hour()), (1, 22));
        assert_eq!(entries[&626].command, "/usr/libexec/rapportd");
    }

    #[test]
    fn bad_lines_are_skipped() {
        assert!(parse("garbage\n\n 12 user\n").is_empty());
        // A date that doesn't parse keeps the rest.
        let entries = parse("7 me Xxx Yyy 99 99:99:99 2026 sleep 5\n");
        assert_eq!(entries[&7].started, None);
        assert_eq!(entries[&7].command, "sleep 5");
    }
}

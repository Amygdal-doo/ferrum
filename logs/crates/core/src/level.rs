//! Log levels and detecting them in free-form text.

use serde::{Deserialize, Serialize};

/// Severity, ordered from least to most severe.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Level {
    Trace,
    Debug,
    Info,
    Warn,
    Error,
}

impl Level {
    pub const ALL: [Level; 5] = [
        Level::Error,
        Level::Warn,
        Level::Info,
        Level::Debug,
        Level::Trace,
    ];

    /// Three-letter column label.
    #[must_use]
    pub fn short(self) -> &'static str {
        match self {
            Level::Trace => "TRC",
            Level::Debug => "DBG",
            Level::Info => "INF",
            Level::Warn => "WRN",
            Level::Error => "ERR",
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Level::Trace => "Trace",
            Level::Debug => "Debug",
            Level::Info => "Info",
            Level::Warn => "Warn",
            Level::Error => "Error",
        }
    }

    /// A level word (`warn`, `WARNING`, `E`, `fatal`, …), any case.
    #[must_use]
    pub fn from_word(word: &str) -> Option<Level> {
        let level = match word.to_ascii_lowercase().as_str() {
            "trace" | "trc" | "t" | "verbose" | "vrb" => Level::Trace,
            "debug" | "dbg" | "d" => Level::Debug,
            "info" | "inf" | "i" | "notice" | "information" => Level::Info,
            "warn" | "wrn" | "w" | "warning" => Level::Warn,
            "error" | "err" | "e" | "fatal" | "ftl" | "panic" | "critical" | "crit" | "crt"
            | "emerg" | "alert" | "severe" => Level::Error,
            _ => return None,
        };
        Some(level)
    }

    /// pino/bunyan numeric levels (10 trace … 50 error, 60 fatal).
    #[must_use]
    pub fn from_number(n: f64) -> Option<Level> {
        Some(match n as i64 {
            ..=10 => Level::Trace,
            11..=20 => Level::Debug,
            21..=30 => Level::Info,
            31..=40 => Level::Warn,
            _ => Level::Error,
        })
    }
}

/// How far into a line level markers are looked for.
const SCAN_BYTES: usize = 160;

/// Guesses the level of a plain-text line:
/// - `level=warn`, `lvl=error`, `"level":"info"` anywhere near the start;
/// - `[E]`, `[WARN]`, `<error>`, `E:` style markers;
/// - an upper-case level word (`ERROR`, `WARN`, …) near the start;
/// - a first word like `error:`/`warning[…]` (cargo, winston, Node errors).
#[must_use]
pub fn detect(text: &str) -> Option<Level> {
    let mut end = text.len().min(SCAN_BYTES);
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    let head = &text[..end];
    if let Some(level) = key_value(head) {
        return Some(level);
    }
    let trimmed = head.trim_start();
    let words = words(trimmed);
    // The first word decides in any case when it is followed by a marker
    // (`error:`, `[warn]`, `info |`, `E1234`-free single letters only bracketed).
    if let Some(&(start, word)) = words.first() {
        let before = &trimmed[..start];
        let after = trimmed[start + word.len()..].chars().next();
        let bracketed = before.ends_with(['[', '<', '(']) && matches!(after, Some(']' | '>' | ')'));
        let marked = matches!(after, Some(':' | '[' | '|')) && word.len() > 1;
        if (bracketed || marked)
            && let Some(level) = Level::from_word(word)
        {
            return Some(level);
        }
    }
    for (start, word) in words {
        let before = &trimmed[..start];
        let after = trimmed[start + word.len()..].chars().next();
        let bracketed = before.ends_with(['[', '<']) && matches!(after, Some(']' | '>'));
        let upper = word.len() > 2 && word.chars().all(|c| c.is_ascii_uppercase());
        if (bracketed || upper)
            && let Some(level) = Level::from_word(word)
        {
            return Some(level);
        }
    }
    None
}

/// `level=x`, `lvl=x`, `severity=x`, `"level":"x"` (also numeric).
fn key_value(head: &str) -> Option<Level> {
    let lower = head.to_ascii_lowercase();
    for key in ["level", "lvl", "severity"] {
        let mut from = 0;
        while let Some(pos) = lower[from..].find(key) {
            let at = from + pos;
            from = at + key.len();
            // Must be a whole key: not `loglevel_x` / `levels`.
            if lower[..at]
                .chars()
                .next_back()
                .is_some_and(|c| c.is_ascii_alphanumeric() || c == '_')
            {
                continue;
            }
            let rest = lower[from..].trim_start_matches('"');
            let Some(rest) = rest.strip_prefix(['=', ':']) else {
                continue;
            };
            let value: String = rest
                .trim_start_matches([' ', '"'])
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric())
                .collect();
            if let Ok(n) = value.parse::<f64>() {
                return Level::from_number(n);
            }
            if let Some(level) = Level::from_word(&value) {
                return Some(level);
            }
        }
    }
    None
}

/// Alphabetic words with their byte offsets.
fn words(text: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in text.char_indices() {
        match (c.is_ascii_alphabetic(), start) {
            (true, None) => start = Some(i),
            (false, Some(s)) => {
                out.push((s, &text[s..i]));
                start = None;
            }
            _ => {}
        }
    }
    if let Some(s) = start {
        out.push((s, &text[s..]));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_common_formats() {
        let cases = [
            ("2024-05-01 10:00:00 ERROR db down", Some(Level::Error)),
            ("[WARN] slow query", Some(Level::Warn)),
            ("[E] boom", Some(Level::Error)),
            ("<info> ready", Some(Level::Info)),
            ("ts=1 level=debug msg=hi", Some(Level::Debug)),
            ("time=x lvl=warn", Some(Level::Warn)),
            (r#"{"level":"error","msg":"x"}"#, Some(Level::Error)),
            (r#"{"level":30,"msg":"x"}"#, Some(Level::Info)),
            ("error[E0425]: cannot find value", Some(Level::Error)),
            ("warning: unused variable", Some(Level::Warn)),
            ("info: server started", Some(Level::Info)),
            ("Error: ENOENT", Some(Level::Error)),
            ("  INFO  [main] started", Some(Level::Info)),
            ("12:00:01 FATAL out of memory", Some(Level::Error)),
            ("[2024-05-01] local.ERROR: x", Some(Level::Error)),
            ("there was no error here", None),
            ("I like it", None),
            ("loglevel=error is configured", None),
            ("GET /health 200", None),
        ];
        for (line, want) in cases {
            assert_eq!(detect(line), want, "{line}");
        }
    }

    #[test]
    fn words_and_numbers() {
        assert_eq!(Level::from_word("WARNING"), Some(Level::Warn));
        assert_eq!(Level::from_word("crit"), Some(Level::Error));
        assert_eq!(Level::from_word("nope"), None);
        assert_eq!(Level::from_number(60.), Some(Level::Error));
        assert_eq!(Level::from_number(20.), Some(Level::Debug));
        assert!(Level::Error > Level::Warn);
        // Multi-byte text near the scan limit does not panic.
        let long = "é".repeat(200);
        assert_eq!(detect(&long), None);
    }
}

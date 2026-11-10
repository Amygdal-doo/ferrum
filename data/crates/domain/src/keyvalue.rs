//! Key-value stores (Redis): key listings and per-key values for the key
//! browser. Values are shaped as grid columns + rows so the results grid
//! renders them like any query result.

use crate::{CellValue, ColumnMeta};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Data type of one key, as reported by `TYPE`.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum KeyKind {
    String,
    Hash,
    List,
    Set,
    SortedSet,
    Stream,
    /// The key no longer exists (`TYPE` answered `none`).
    Missing,
    /// A module type (e.g. `ReJSON-RL`) or anything newer than this enum.
    Other(String),
}

impl KeyKind {
    #[must_use]
    pub fn from_type_name(name: &str) -> Self {
        match name.to_ascii_lowercase().as_str() {
            "string" => Self::String,
            "hash" => Self::Hash,
            "list" => Self::List,
            "set" => Self::Set,
            "zset" => Self::SortedSet,
            "stream" => Self::Stream,
            "none" => Self::Missing,
            _ => Self::Other(name.to_owned()),
        }
    }

    /// Short badge text for the tree, e.g. `str`, `zset`.
    #[must_use]
    pub fn badge(&self) -> &str {
        match self {
            Self::String => "str",
            Self::Hash => "hash",
            Self::List => "list",
            Self::Set => "set",
            Self::SortedSet => "zset",
            Self::Stream => "stream",
            Self::Missing => "gone",
            Self::Other(name) => name,
        }
    }

    #[must_use]
    pub fn label(&self) -> &str {
        match self {
            Self::String => "string",
            Self::Hash => "hash",
            Self::List => "list",
            Self::Set => "set",
            Self::SortedSet => "sorted set",
            Self::Stream => "stream",
            Self::Missing => "missing",
            Self::Other(name) => name,
        }
    }
}

/// Remaining time to live of a key.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyTtl {
    /// No expiry set.
    Persistent,
    ExpiresIn(Duration),
    /// The key does not exist (any more).
    Missing,
}

impl KeyTtl {
    /// From a `PTTL` reply: `-1` = no expiry, `-2` = no such key.
    #[must_use]
    pub fn from_pttl(pttl: i64) -> Self {
        match u64::try_from(pttl) {
            Ok(ms) => Self::ExpiresIn(Duration::from_millis(ms)),
            Err(_) if pttl == -1 => Self::Persistent,
            Err(_) => Self::Missing,
        }
    }

    /// `no TTL`, `TTL 4m 10s`, `gone`.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Persistent => "no TTL".into(),
            Self::ExpiresIn(d) => format!("TTL {}", format_duration(*d)),
            Self::Missing => "gone".into(),
        }
    }
}

/// Compact duration: `850ms`, `42s`, `3m 12s`, `2h 5m`, `3d 4h`.
#[must_use]
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs();
    let (days, hours, mins, s) = (secs / 86_400, secs / 3_600 % 24, secs / 60 % 60, secs % 60);
    if secs == 0 {
        format!("{}ms", d.as_millis())
    } else if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {mins}m")
    } else if mins > 0 {
        format!("{mins}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// `512 B`, `1.5 KiB`, `12.0 MiB`.
#[must_use]
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KiB", "MiB", "GiB", "TiB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    #[allow(clippy::cast_precision_loss)]
    let mut value = bytes as f64 / 1024.0;
    let mut unit = 0;
    while value >= 1024.0 && unit + 1 < UNITS.len() {
        value /= 1024.0;
        unit += 1;
    }
    format!("{value:.1} {}", UNITS[unit])
}

/// One key from a `SCAN` page with its type and TTL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct KeyEntry {
    /// Key name; lossy UTF-8 when `binary` is set.
    pub name: String,
    pub kind: KeyKind,
    pub ttl: KeyTtl,
    /// The key is not valid UTF-8: it is listed but cannot be opened or edited.
    pub binary: bool,
}

/// One `SCAN` page. `cursor == 0` means the iteration is complete.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct KeyScanPage {
    pub keys: Vec<KeyEntry>,
    pub cursor: u64,
}

impl KeyScanPage {
    #[must_use]
    pub const fn is_complete(&self) -> bool {
        self.cursor == 0
    }
}

/// A key's metadata and value, shaped for the results grid.
#[derive(Debug, Clone, PartialEq)]
pub struct KeyValueView {
    pub key: String,
    pub kind: KeyKind,
    pub ttl: KeyTtl,
    /// `MEMORY USAGE`; `None` if the server does not allow or support it.
    pub memory_bytes: Option<u64>,
    /// Element count (fields, items, members, entries) or string length in bytes.
    pub length: Option<u64>,
    /// Text of a string value (pretty-printed when it is JSON); `None` for
    /// other types and for binary strings.
    pub text: Option<String>,
    pub is_json: bool,
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<CellValue>>,
    /// Fewer rows (or string bytes) than the key holds were fetched.
    pub truncated: bool,
}

impl KeyValueView {
    /// `hash · 12 fields · TTL 4m 10s · 1.2 KiB` style summary.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = vec![self.kind.label().to_owned()];
        if let Some(n) = self.length {
            let unit = match self.kind {
                KeyKind::String => "bytes",
                KeyKind::Hash => "fields",
                KeyKind::List => "items",
                KeyKind::Set | KeyKind::SortedSet => "members",
                KeyKind::Stream => "entries",
                KeyKind::Missing | KeyKind::Other(_) => "elements",
            };
            parts.push(format!("{n} {unit}"));
        }
        if self.is_json {
            parts.push("JSON".into());
        }
        parts.push(self.ttl.label());
        if let Some(m) = self.memory_bytes {
            parts.push(format!("{} in memory", format_bytes(m)));
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn type_names_and_ttl_replies() {
        assert_eq!(KeyKind::from_type_name("zset"), KeyKind::SortedSet);
        assert_eq!(KeyKind::from_type_name("none"), KeyKind::Missing);
        assert_eq!(
            KeyKind::from_type_name("ReJSON-RL"),
            KeyKind::Other("ReJSON-RL".into())
        );
        assert_eq!(KeyTtl::from_pttl(-1), KeyTtl::Persistent);
        assert_eq!(KeyTtl::from_pttl(-2), KeyTtl::Missing);
        assert_eq!(
            KeyTtl::from_pttl(1500),
            KeyTtl::ExpiresIn(Duration::from_millis(1500))
        );
        assert_eq!(KeyTtl::from_pttl(250_000).label(), "TTL 4m 10s");
    }

    #[test]
    fn durations_and_sizes() {
        assert_eq!(format_duration(Duration::from_millis(850)), "850ms");
        assert_eq!(format_duration(Duration::from_secs(42)), "42s");
        assert_eq!(format_duration(Duration::from_secs(7_500)), "2h 5m");
        assert_eq!(
            format_duration(Duration::from_secs(3 * 86_400 + 4 * 3_600)),
            "3d 4h"
        );
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1536), "1.5 KiB");
        assert_eq!(format_bytes(12 * 1024 * 1024), "12.0 MiB");
    }

    #[test]
    fn view_summary() {
        let v = KeyValueView {
            key: "k".into(),
            kind: KeyKind::Hash,
            ttl: KeyTtl::Persistent,
            memory_bytes: Some(2048),
            length: Some(3),
            text: None,
            is_json: false,
            columns: vec![],
            rows: vec![],
            truncated: false,
        };
        assert_eq!(v.summary(), "hash · 3 fields · no TTL · 2.0 KiB in memory");
    }
}

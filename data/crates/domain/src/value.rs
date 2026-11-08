//! Typed result cells and column metadata.

use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime};
use serde::{Deserialize, Serialize};

/// Engine-independent classification used for rendering and editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LogicalType {
    Boolean,
    Integer,
    Float,
    Decimal,
    Text,
    Binary,
    Date,
    Time,
    Timestamp,
    TimestampTz,
    Json,
    Uuid,
    Other,
}

impl LogicalType {
    #[must_use]
    pub const fn is_numeric(self) -> bool {
        matches!(self, Self::Integer | Self::Float | Self::Decimal)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnMeta {
    pub name: String,
    /// Type name as reported by the engine, e.g. `int4`, `timestamptz`.
    pub db_type: String,
    pub logical_type: LogicalType,
    pub nullable: Option<bool>,
    pub primary_key: bool,
    pub editable: bool,
}

/// One cell. Decimals stay as strings to avoid lossy float conversion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CellValue {
    Null,
    Bool(bool),
    Signed(i64),
    Unsigned(u64),
    Float(f64),
    Decimal(String),
    Text(String),
    Binary(Vec<u8>),
    Date(NaiveDate),
    Time(NaiveTime),
    Timestamp(NaiveDateTime),
    TimestampTz(DateTime<FixedOffset>),
    Json(String),
    Uuid(String),
    /// Value the adapter cannot decode yet; carries the engine type name.
    Unsupported(String),
}

impl CellValue {
    /// Max characters rendered inline in a grid cell.
    pub const DISPLAY_LIMIT: usize = 256;

    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self, Self::Null)
    }

    /// Plain-text rendering for the grid. Never interpreted as HTML by the UI.
    #[must_use]
    pub fn display(&self) -> String {
        let full = match self {
            Self::Null => return "NULL".to_owned(),
            Self::Bool(b) => b.to_string(),
            Self::Signed(v) => v.to_string(),
            Self::Unsigned(v) => v.to_string(),
            Self::Float(v) => v.to_string(),
            Self::Decimal(v) | Self::Text(v) | Self::Json(v) | Self::Uuid(v) => v.clone(),
            Self::Binary(bytes) => return binary_preview(bytes),
            Self::Date(d) => d.to_string(),
            Self::Time(t) => t.to_string(),
            Self::Timestamp(ts) => ts.to_string(),
            Self::TimestampTz(ts) => ts.to_rfc3339(),
            Self::Unsupported(ty) => return format!("<{ty}>"),
        };
        truncate_chars(&full, Self::DISPLAY_LIMIT)
    }
}

fn binary_preview(bytes: &[u8]) -> String {
    const PREVIEW: usize = 16;
    let hex: String = bytes
        .iter()
        .take(PREVIEW)
        .map(|b| format!("{b:02x}"))
        .collect();
    let ellipsis = if bytes.len() > PREVIEW { "…" } else { "" };
    format!("0x{hex}{ellipsis} ({} bytes)", bytes.len())
}

fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => format!("{}…", &s[..idx]),
        None => s.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn null_differs_from_empty_text() {
        assert_eq!(CellValue::Null.display(), "NULL");
        assert_eq!(CellValue::Text(String::new()).display(), "");
        assert_ne!(CellValue::Null, CellValue::Text(String::new()));
    }

    #[test]
    fn truncation_respects_char_boundaries() {
        let s = "é".repeat(300);
        let shown = CellValue::Text(s).display();
        assert_eq!(shown.chars().count(), CellValue::DISPLAY_LIMIT + 1);
    }

    #[test]
    fn binary_is_previewed_as_hex() {
        assert_eq!(
            CellValue::Binary(vec![0xde, 0xad]).display(),
            "0xdead (2 bytes)"
        );
    }
}

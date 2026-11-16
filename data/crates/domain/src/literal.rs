//! SQL literals for statements FerrumData generates itself (row edits, CSV
//! import, SQL dumps, migrations). Every value is written as a literal of
//! the engine's dialect; nothing here is used to sanitise user SQL.

use crate::{CellValue, DatabaseEngine};
use std::fmt::Write as _;

impl DatabaseEngine {
    /// A string literal. PostgreSQL uses `E'…'` only when the text has a
    /// backslash (so `standard_conforming_strings` does not matter); MySQL
    /// escapes backslashes because they are escape characters by default.
    #[must_use]
    pub fn string_literal(self, text: &str) -> String {
        match self {
            Self::Postgres if text.contains('\\') => {
                format!("E'{}'", text.replace('\\', "\\\\").replace('\'', "''"))
            }
            Self::MySql => {
                let mut out = String::with_capacity(text.len() + 2);
                out.push('\'');
                for c in text.chars() {
                    match c {
                        '\\' => out.push_str("\\\\"),
                        '\'' => out.push_str("''"),
                        '\0' => out.push_str("\\0"),
                        c => out.push(c),
                    }
                }
                out.push('\'');
                out
            }
            _ => format!("'{}'", text.replace('\'', "''")),
        }
    }

    /// The literal for a decoded cell, or `None` for a value FerrumData
    /// could not decode (it cannot be written back faithfully).
    #[must_use]
    pub fn literal(self, value: &CellValue) -> Option<String> {
        Some(match value {
            CellValue::Null => "NULL".to_owned(),
            CellValue::Bool(b) => match (self, b) {
                (Self::Sqlite, true) => "1".to_owned(),
                (Self::Sqlite, false) => "0".to_owned(),
                (_, true) => "TRUE".to_owned(),
                (_, false) => "FALSE".to_owned(),
            },
            CellValue::Signed(v) => v.to_string(),
            CellValue::Unsigned(v) => v.to_string(),
            CellValue::Float(v) if v.is_finite() => format!("{v:?}"),
            // NaN / ±Infinity only exist as text in SQL.
            CellValue::Float(v) => self.string_literal(&v.to_string()),
            CellValue::Decimal(v) => v.clone(),
            CellValue::Text(v) | CellValue::Json(v) | CellValue::Uuid(v) => self.string_literal(v),
            CellValue::Binary(bytes) => self.binary_literal(bytes),
            CellValue::Date(d) => self.string_literal(&d.to_string()),
            CellValue::Time(t) => self.string_literal(&t.to_string()),
            CellValue::Timestamp(ts) => self.string_literal(&ts.to_string()),
            CellValue::TimestampTz(ts) => self.string_literal(&ts.to_rfc3339()),
            CellValue::Unsupported(_) => return None,
        })
    }

    fn binary_literal(self, bytes: &[u8]) -> String {
        let mut hex = String::with_capacity(bytes.len() * 2);
        for b in bytes {
            let _ = write!(hex, "{b:02x}");
        }
        match self {
            Self::Postgres => format!("'\\x{hex}'::bytea"),
            _ => format!("X'{hex}'"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strings_escape_per_dialect() {
        assert_eq!(DatabaseEngine::Postgres.string_literal("it's"), "'it''s'");
        assert_eq!(
            DatabaseEngine::Postgres.string_literal(r"a\b'"),
            r"E'a\\b'''"
        );
        assert_eq!(DatabaseEngine::MySql.string_literal(r"a\b'"), r"'a\\b'''");
        assert_eq!(DatabaseEngine::Sqlite.string_literal(r"a\b'"), r"'a\b'''");
    }

    #[test]
    fn values_render_as_literals() {
        let pg = DatabaseEngine::Postgres;
        assert_eq!(pg.literal(&CellValue::Null).as_deref(), Some("NULL"));
        assert_eq!(pg.literal(&CellValue::Signed(-3)).as_deref(), Some("-3"));
        assert_eq!(pg.literal(&CellValue::Float(1.0)).as_deref(), Some("1.0"));
        assert_eq!(
            pg.literal(&CellValue::Float(f64::NAN)).as_deref(),
            Some("'NaN'")
        );
        assert_eq!(
            pg.literal(&CellValue::Binary(vec![0xab, 1])).as_deref(),
            Some("'\\xab01'::bytea")
        );
        assert_eq!(
            DatabaseEngine::Sqlite
                .literal(&CellValue::Binary(vec![0xab]))
                .as_deref(),
            Some("X'ab'")
        );
        assert_eq!(
            DatabaseEngine::Sqlite
                .literal(&CellValue::Bool(true))
                .as_deref(),
            Some("1")
        );
        assert_eq!(pg.literal(&CellValue::Unsupported("x".into())), None);
    }
}

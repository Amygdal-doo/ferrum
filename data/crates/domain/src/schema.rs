//! Database object metadata used by the explorer and table browser.

use crate::DatabaseEngine;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableKind {
    Table,
    View,
    MaterializedView,
    ForeignTable,
    PartitionedTable,
}

impl TableKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Table => "table",
            Self::View => "view",
            Self::MaterializedView => "materialized view",
            Self::ForeignTable => "foreign table",
            Self::PartitionedTable => "partitioned table",
        }
    }
}

/// A table-like object, identified exactly as stored (case preserved).
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TableRef {
    pub schema: Option<String>,
    pub name: String,
}

impl TableRef {
    /// Fully qualified, dialect-quoted name, e.g. `"public"."User"`.
    #[must_use]
    pub fn qualified(&self, engine: DatabaseEngine) -> String {
        match &self.schema {
            Some(s) => format!(
                "{}.{}",
                engine.quote_ident(s),
                engine.quote_ident(&self.name)
            ),
            None => engine.quote_ident(&self.name),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableInfo {
    pub table: TableRef,
    pub kind: TableKind,
    /// Primary-key columns in key order; empty if none.
    pub primary_key: Vec<String>,
    /// Planner estimate; `None` if unknown (never analysed).
    pub estimated_rows: Option<i64>,
}

impl DatabaseEngine {
    /// Quotes an identifier so it is used verbatim (case, spaces, keywords).
    #[must_use]
    pub fn quote_ident(self, ident: &str) -> String {
        match self {
            Self::Postgres | Self::Sqlite => format!("\"{}\"", ident.replace('"', "\"\"")),
            Self::MySql => format!("`{}`", ident.replace('`', "``")),
            // SurrealQL backtick identifiers escape with a backslash.
            Self::SurrealDb => format!("`{}`", ident.replace('\\', "\\\\").replace('`', "\\`")),
            // Collection names are JSON strings in the MongoDB query syntax.
            Self::MongoDb => json_string(ident),
        }
    }
}

/// Minimal JSON string literal (quotes, backslashes and control characters escaped).
fn json_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            c if u32::from(c) < 0x20 => out.push_str(&format!("\\u{:04x}", u32::from(c))),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_document_engine_identifiers() {
        assert_eq!(
            DatabaseEngine::SurrealDb.quote_ident("my`tab"),
            "`my\\`tab`"
        );
        assert_eq!(DatabaseEngine::MongoDb.quote_ident("a\"b"), "\"a\\\"b\"");
    }

    #[test]
    fn quotes_case_keywords_and_embedded_quotes() {
        let pg = DatabaseEngine::Postgres;
        assert_eq!(pg.quote_ident("User"), "\"User\"");
        assert_eq!(pg.quote_ident("we\"ird"), "\"we\"\"ird\"");
        assert_eq!(DatabaseEngine::MySql.quote_ident("a`b"), "`a``b`");
    }

    #[test]
    fn qualified_name() {
        let t = TableRef {
            schema: Some("public".into()),
            name: "User".into(),
        };
        assert_eq!(t.qualified(DatabaseEngine::Postgres), "\"public\".\"User\"");
        let t = TableRef {
            schema: None,
            name: "x".into(),
        };
        assert_eq!(t.qualified(DatabaseEngine::Sqlite), "\"x\"");
    }
}

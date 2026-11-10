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

/// One column of a table, as described by the engine's catalog.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnInfo {
    pub name: String,
    /// Declared type as the engine prints it, e.g. `character varying(80)`.
    pub data_type: String,
    pub nullable: bool,
    /// Default expression as SQL text; `None` if there is none.
    pub default: Option<String>,
    pub primary_key: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IndexInfo {
    pub name: String,
    /// Key columns (or expressions) in index order.
    pub columns: Vec<String>,
    pub unique: bool,
    pub primary: bool,
}

/// Structure of one table for the explorer tree (SQL engines).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TableDetails {
    /// In declaration order.
    pub columns: Vec<ColumnInfo>,
    /// Primary key first, then by name.
    pub indexes: Vec<IndexInfo>,
}

impl ColumnInfo {
    /// Compact tree label suffix, e.g. `int4 · PK`, `text · NULL · = 'x'::text`.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut parts = vec![self.data_type.clone()];
        if self.primary_key {
            parts.push("PK".into());
        }
        parts.push(if self.nullable { "NULL" } else { "NOT NULL" }.into());
        if let Some(d) = &self.default {
            parts.push(format!("= {d}"));
        }
        parts.join(" · ")
    }
}

impl IndexInfo {
    /// e.g. `UNIQUE (email)`, `PRIMARY KEY (id)`, `(lower(name), id)`.
    #[must_use]
    pub fn summary(&self) -> String {
        let kind = if self.primary {
            "PRIMARY KEY "
        } else if self.unique {
            "UNIQUE "
        } else {
            ""
        };
        format!("{kind}({})", self.columns.join(", "))
    }
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
            // Redis console arguments: redis-cli double-quoted string.
            Self::Redis => redis_quote(ident),
        }
    }
}

/// A Redis console argument: bare when it is safe to, otherwise a
/// redis-cli style double-quoted string (`\"`, `\\`, `\n`, `\xHH` escapes).
#[must_use]
pub fn redis_quote(arg: &str) -> String {
    let bare = !arg.is_empty()
        && arg
            .chars()
            .all(|c| !c.is_whitespace() && !c.is_control() && !matches!(c, '"' | '\'' | '\\'));
    if bare {
        return arg.to_owned();
    }
    let mut out = String::with_capacity(arg.len() + 2);
    out.push('"');
    for c in arg.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_ascii_control() => {
                out.push_str(&format!("\\x{:02x}", u32::from(c)));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
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
    fn redis_arguments_are_bare_or_double_quoted() {
        assert_eq!(redis_quote("user:1"), "user:1");
        assert_eq!(redis_quote("a b"), "\"a b\"");
        assert_eq!(redis_quote(""), "\"\"");
        assert_eq!(redis_quote("q\"x\\\n"), "\"q\\\"x\\\\\\n\"");
        assert_eq!(redis_quote("it's"), "\"it's\"");
        assert_eq!(redis_quote("\u{1}"), "\"\\x01\"");
    }

    #[test]
    fn quotes_case_keywords_and_embedded_quotes() {
        let pg = DatabaseEngine::Postgres;
        assert_eq!(pg.quote_ident("User"), "\"User\"");
        assert_eq!(pg.quote_ident("we\"ird"), "\"we\"\"ird\"");
        assert_eq!(DatabaseEngine::MySql.quote_ident("a`b"), "`a``b`");
    }

    #[test]
    fn detail_summaries() {
        let c = ColumnInfo {
            name: "id".into(),
            data_type: "int4".into(),
            nullable: false,
            default: Some("nextval('t_id_seq'::regclass)".into()),
            primary_key: true,
        };
        assert_eq!(
            c.summary(),
            "int4 · PK · NOT NULL · = nextval('t_id_seq'::regclass)"
        );
        let i = IndexInfo {
            name: "t_email".into(),
            columns: vec!["lower(email)".into(), "org".into()],
            unique: true,
            primary: false,
        };
        assert_eq!(i.summary(), "UNIQUE (lower(email), org)");
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

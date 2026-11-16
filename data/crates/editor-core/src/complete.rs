//! Schema-aware completion at the caret: keywords, functions, tables and the
//! columns of the tables a statement names (`alias.` resolves the alias).
//! Lexical like the rest of this crate: it reads the tokens around the caret
//! and the statement's FROM / JOIN / UPDATE / INTO clauses, nothing more.

use crate::keywords::{FUNCTIONS, KEYWORDS};
use crate::lex::{Dialect, Token, TokenKind, tokenize};
use std::ops::Range;

/// Most items returned for one request.
pub const MAX_ITEMS: usize = 100;

/// One table (or view) the completion knows about. `columns` is `None` until
/// the UI has loaded them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatalogTable {
    pub schema: Option<String>,
    pub name: String,
    pub columns: Option<Vec<String>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Catalog {
    pub tables: Vec<CatalogTable>,
}

impl Catalog {
    /// Exact match first, then case-insensitive (bare names fold).
    #[must_use]
    pub fn find(&self, schema: Option<&str>, name: &str) -> Option<&CatalogTable> {
        let schema_ok = |t: &CatalogTable, exact: bool| match (schema, &t.schema) {
            (None, _) => true,
            (Some(s), Some(ts)) => {
                if exact {
                    s == ts
                } else {
                    s.eq_ignore_ascii_case(ts)
                }
            }
            (Some(_), None) => false,
        };
        self.tables
            .iter()
            .find(|t| t.name == name && schema_ok(t, true))
            .or_else(|| {
                self.tables
                    .iter()
                    .find(|t| t.name.eq_ignore_ascii_case(name) && schema_ok(t, false))
            })
    }
}

/// What an item inserts. Ordered by how early it is listed in expressions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum CompletionKind {
    Column,
    Table,
    Schema,
    Function,
    Keyword,
}

impl CompletionKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Column => "column",
            Self::Table => "table",
            Self::Schema => "schema",
            Self::Function => "function",
            Self::Keyword => "keyword",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItem {
    pub label: String,
    /// Text that replaces [`Completions::replace`].
    pub insert: String,
    pub kind: CompletionKind,
    /// e.g. the table a column belongs to.
    pub detail: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completions {
    /// The partial word before the caret (empty right after `.` or a space).
    pub replace: Range<usize>,
    pub items: Vec<CompletionItem>,
}

/// A table named by a statement, with its alias if it has one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableMention {
    pub schema: Option<String>,
    pub name: String,
    pub alias: Option<String>,
}

/// Tables named in the FROM / JOIN / UPDATE / INTO clauses of the statement
/// around `caret`, so the UI can load their columns before completing them.
#[must_use]
pub fn mentions_at(sql: &str, caret: usize, dialect: Dialect) -> Vec<TableMention> {
    let tokens = tokenize(sql, dialect);
    let range = statement_tokens(sql, &tokens, caret);
    table_mentions(sql, &tokens[range])
}

/// Completions at `caret`, or `None` inside a string or comment.
#[must_use]
pub fn complete(
    sql: &str,
    caret: usize,
    dialect: Dialect,
    catalog: &Catalog,
) -> Option<Completions> {
    let caret = caret.min(sql.len());
    let tokens = tokenize(sql, dialect);
    if tokens.iter().any(|t| inside_literal(sql, t, caret)) {
        return None;
    }
    let range = statement_tokens(sql, &tokens, caret);
    let statement = &tokens[range];
    // Tokens ending at or before the caret; the last one is the partial
    // word being typed when it touches the caret.
    let at = statement.partition_point(|t| t.range.end <= caret);
    let partial = statement[..at]
        .last()
        .filter(|t| t.is_word() && t.range.end == caret);
    let replace = partial.map_or(caret..caret, |t| t.range.start..caret);
    let prefix = &sql[replace.clone()];
    let before = &statement[..at - usize::from(partial.is_some())];

    let items = match qualifier(sql, before) {
        Some(q) => qualified_items(&q, &table_mentions(sql, statement), catalog, dialect),
        None if table_context(sql, before) => table_items(catalog, dialect),
        None => {
            let mentions = table_mentions(sql, statement);
            let mut items = mention_columns(&mentions, catalog, dialect);
            items.extend(table_items(catalog, dialect));
            items.extend(word_items(FUNCTIONS, CompletionKind::Function, prefix));
            items.extend(word_items(KEYWORDS, CompletionKind::Keyword, prefix));
            items
        }
    };
    Some(Completions {
        replace,
        items: rank(items, prefix),
    })
}

/// Token index range of the statement containing `caret` (`;`-separated).
fn statement_tokens(sql: &str, tokens: &[Token], caret: usize) -> Range<usize> {
    let is_semi = |t: &Token| t.kind == TokenKind::Punctuation && t.text(sql) == ";";
    let start = tokens
        .iter()
        .rposition(|t| is_semi(t) && t.range.end <= caret)
        .map_or(0, |i| i + 1);
    let end = tokens[start..]
        .iter()
        .position(is_semi)
        .map_or(tokens.len(), |p| start + p);
    start..end
}

/// The caret is inside a string, comment or quoted identifier.
fn inside_literal(sql: &str, t: &Token, caret: usize) -> bool {
    let literal = matches!(
        t.kind,
        TokenKind::String | TokenKind::Comment | TokenKind::QuotedIdentifier
    );
    if !literal || caret <= t.range.start || caret > t.range.end {
        return false;
    }
    if caret < t.range.end {
        return true;
    }
    // At the end: still inside a line comment or an unterminated literal.
    let text = t.text(sql);
    match t.kind {
        TokenKind::Comment => !text.starts_with("/*") || !text.ends_with("*/") || text.len() < 4,
        _ => {
            let open = text.trim_start_matches(|c: char| c.is_ascii_alphabetic());
            let quote = open.chars().next();
            let close = match quote {
                Some('[') => Some(']'),
                Some('$') => Some('$'),
                q => q,
            };
            open.len() < 2 || open.chars().last() != close
        }
    }
}

/// `x.` / `x.par` before the caret: the qualifier's name.
fn qualifier(sql: &str, before: &[Token]) -> Option<String> {
    let [.., name, dot] = before else {
        return None;
    };
    let is_dot = dot.kind == TokenKind::Punctuation && dot.text(sql) == ".";
    let named = name.is_word() || name.kind == TokenKind::QuotedIdentifier;
    (is_dot && named && name.range.end == dot.range.start).then(|| name.ident_name(sql))
}

/// Right after FROM / JOIN / UPDATE / INTO / TABLE, or a comma in a FROM list.
fn table_context(sql: &str, before: &[Token]) -> bool {
    const TABLE_KEYWORDS: [&str; 5] = ["FROM", "JOIN", "UPDATE", "INTO", "TABLE"];
    let upper = |t: &Token| t.text(sql).to_ascii_uppercase();
    let Some(last) = before.last() else {
        return false;
    };
    if last.kind == TokenKind::Keyword {
        return TABLE_KEYWORDS.contains(&upper(last).as_str());
    }
    if last.text(sql) != "," {
        return false;
    }
    // A comma continues a FROM list when FROM is the nearest clause keyword.
    let mut depth = 0i32;
    for t in before.iter().rev() {
        match t.text(sql) {
            ")" => depth += 1,
            "(" => depth -= 1,
            _ if depth == 0 && t.kind == TokenKind::Keyword => {
                let word = upper(t);
                if matches!(word.as_str(), "AS" | "ON" | "AND" | "OR" | "NOT") {
                    continue;
                }
                return word == "FROM";
            }
            _ => {}
        }
    }
    false
}

fn table_mentions(sql: &str, statement: &[Token]) -> Vec<TableMention> {
    let name_token = |t: &Token| {
        t.kind == TokenKind::QuotedIdentifier
            || matches!(
                t.kind,
                TokenKind::Identifier | TokenKind::Function | TokenKind::Type
            )
    };
    let mut out = Vec::new();
    let mut i = 0;
    while i < statement.len() {
        let word = statement[i].text(sql).to_ascii_uppercase();
        let from = statement[i].kind == TokenKind::Keyword && word == "FROM";
        if statement[i].kind != TokenKind::Keyword
            || !matches!(word.as_str(), "FROM" | "JOIN" | "UPDATE" | "INTO")
        {
            i += 1;
            continue;
        }
        i += 1;
        while let Some(first) = statement.get(i).filter(|t| name_token(t)) {
            let mut schema = None;
            let mut name = first.ident_name(sql);
            i += 1;
            let dotted = statement.get(i).is_some_and(|t| t.text(sql) == ".");
            if dotted && let Some(second) = statement.get(i + 1).filter(|t| name_token(t)) {
                schema = Some(name);
                name = second.ident_name(sql);
                i += 2;
            }
            if statement
                .get(i)
                .is_some_and(|t| t.text(sql).eq_ignore_ascii_case("AS"))
            {
                i += 1;
            }
            let alias = statement
                .get(i)
                .filter(|t| matches!(t.kind, TokenKind::Identifier | TokenKind::QuotedIdentifier))
                .map(|t| t.ident_name(sql));
            if alias.is_some() {
                i += 1;
            }
            out.push(TableMention {
                schema,
                name,
                alias,
            });
            if from && statement.get(i).is_some_and(|t| t.text(sql) == ",") {
                i += 1;
                continue;
            }
            break;
        }
    }
    out
}

/// Columns of the aliased / named table, or the tables of a schema.
fn qualified_items(
    qualifier: &str,
    mentions: &[TableMention],
    catalog: &Catalog,
    dialect: Dialect,
) -> Vec<CompletionItem> {
    let by_alias = mentions.iter().find(|m| {
        m.alias
            .as_deref()
            .is_some_and(|a| a.eq_ignore_ascii_case(qualifier))
    });
    let by_name = || {
        mentions
            .iter()
            .find(|m| m.name.eq_ignore_ascii_case(qualifier))
    };
    let table = match by_alias.or_else(by_name) {
        Some(m) => catalog.find(m.schema.as_deref(), &m.name),
        None => catalog.find(None, qualifier),
    };
    if let Some(table) = table {
        return columns_of(table, dialect);
    }
    catalog
        .tables
        .iter()
        .filter(|t| {
            t.schema
                .as_deref()
                .is_some_and(|s| s.eq_ignore_ascii_case(qualifier))
        })
        .map(|t| CompletionItem {
            label: t.name.clone(),
            insert: dialect.ident(&t.name),
            kind: CompletionKind::Table,
            detail: None,
        })
        .collect()
}

fn columns_of(table: &CatalogTable, dialect: Dialect) -> Vec<CompletionItem> {
    table
        .columns
        .iter()
        .flatten()
        .map(|c| CompletionItem {
            label: c.clone(),
            insert: dialect.ident(c),
            kind: CompletionKind::Column,
            detail: Some(table.name.clone()),
        })
        .collect()
}

fn mention_columns(
    mentions: &[TableMention],
    catalog: &Catalog,
    dialect: Dialect,
) -> Vec<CompletionItem> {
    mentions
        .iter()
        .filter_map(|m| catalog.find(m.schema.as_deref(), &m.name))
        .flat_map(|t| columns_of(t, dialect))
        .collect()
}

/// Every table, schema-qualified when the bare name would be ambiguous or
/// (PostgreSQL) outside `public`; plus each schema name.
fn table_items(catalog: &Catalog, dialect: Dialect) -> Vec<CompletionItem> {
    let mut items: Vec<CompletionItem> = catalog
        .tables
        .iter()
        .map(|t| {
            let ambiguous = catalog
                .tables
                .iter()
                .filter(|o| o.name.eq_ignore_ascii_case(&t.name))
                .count()
                > 1;
            let qualify = t
                .schema
                .as_deref()
                .is_some_and(|s| ambiguous || (dialect == Dialect::Postgres && s != "public"));
            let insert = match (&t.schema, qualify) {
                (Some(s), true) => format!("{}.{}", dialect.ident(s), dialect.ident(&t.name)),
                _ => dialect.ident(&t.name),
            };
            CompletionItem {
                label: t.name.clone(),
                insert,
                kind: CompletionKind::Table,
                detail: t.schema.clone(),
            }
        })
        .collect();
    let mut schemas: Vec<&str> = catalog
        .tables
        .iter()
        .filter_map(|t| t.schema.as_deref())
        .collect();
    schemas.sort_unstable();
    schemas.dedup();
    if schemas.len() > 1 {
        items.extend(schemas.into_iter().map(|s| CompletionItem {
            label: s.to_owned(),
            insert: dialect.ident(s),
            kind: CompletionKind::Schema,
            detail: None,
        }));
    }
    items
}

/// Keywords / functions in the case the user is typing in.
fn word_items(words: &[&str], kind: CompletionKind, prefix: &str) -> Vec<CompletionItem> {
    let lower = !prefix.is_empty() && !prefix.chars().any(|c| c.is_ascii_uppercase());
    words
        .iter()
        .map(|w| {
            let text = if lower {
                w.to_ascii_lowercase()
            } else {
                (*w).to_owned()
            };
            CompletionItem {
                label: text.clone(),
                insert: text,
                kind,
                detail: None,
            }
        })
        .collect()
}

/// Case-insensitive prefix filter (the item's own text already typed is
/// dropped), deduplicated, ordered by kind then name, capped.
fn rank(items: Vec<CompletionItem>, prefix: &str) -> Vec<CompletionItem> {
    let prefix = prefix.to_lowercase();
    let mut out: Vec<CompletionItem> = items
        .into_iter()
        .filter(|i| {
            let label = i.label.to_lowercase();
            label.starts_with(&prefix) && label != prefix
        })
        .collect();
    out.sort_by(|a, b| {
        a.kind
            .cmp(&b.kind)
            .then_with(|| a.label.to_lowercase().cmp(&b.label.to_lowercase()))
            .then_with(|| a.detail.cmp(&b.detail))
    });
    out.dedup_by(|a, b| a.kind == b.kind && a.insert == b.insert);
    out.truncate(MAX_ITEMS);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog() -> Catalog {
        let table = |schema: &str, name: &str, cols: &[&str]| CatalogTable {
            schema: Some(schema.into()),
            name: name.into(),
            columns: Some(cols.iter().map(|c| (*c).to_owned()).collect()),
        };
        Catalog {
            tables: vec![
                table("public", "users", &["id", "email", "Name"]),
                table("public", "orders", &["id", "user_id", "total"]),
                table("audit", "log", &["at", "what"]),
            ],
        }
    }

    fn labels(sql: &str) -> Vec<String> {
        let caret = sql.find('|').expect("caret marker");
        let doc = sql.replace('|', "");
        complete(&doc, caret, Dialect::Postgres, &catalog())
            .map(|c| c.items.into_iter().map(|i| i.label).collect())
            .unwrap_or_default()
    }

    #[test]
    fn alias_dot_lists_that_tables_columns() {
        assert_eq!(
            labels("SELECT o.| FROM orders o JOIN users AS u ON u.id = o.user_id"),
            vec!["id", "total", "user_id"]
        );
        assert_eq!(labels("SELECT u.e| FROM users u"), vec!["email"]);
        // A table name works as its own qualifier.
        assert_eq!(labels("SELECT users.N| FROM users"), vec!["Name"]);
    }

    #[test]
    fn after_from_and_join_only_tables_are_offered() {
        assert_eq!(labels("SELECT * FROM o|"), vec!["orders"]);
        assert_eq!(
            labels("SELECT * FROM users u JOIN |"),
            vec!["log", "orders", "users", "audit", "public"]
        );
        assert_eq!(labels("SELECT * FROM users, ord|"), vec!["orders"]);
    }

    #[test]
    fn expressions_offer_columns_of_named_tables_then_keywords() {
        let got = labels("SELECT * FROM users WHERE e|");
        assert_eq!(got.first().map(String::as_str), Some("email"));
        assert!(
            got.contains(&"exists".to_owned()),
            "lower-case keywords: {got:?}"
        );
        assert!(!got.contains(&"user_id".to_owned()), "orders is not named");
    }

    #[test]
    fn nothing_inside_strings_or_comments() {
        assert!(labels("SELECT 'us|'").is_empty());
        assert!(labels("SELECT 1 -- us|").is_empty());
        assert!(!labels("SELECT 'x' || us|").is_empty());
    }

    #[test]
    fn only_the_statement_at_the_caret_counts() {
        assert_eq!(
            labels("SELECT * FROM orders; SELECT n| FROM users")[0],
            "Name"
        );
        assert!(!labels("SELECT * FROM orders; SELECT t| FROM users").contains(&"total".into()));
    }

    #[test]
    fn inserts_quote_and_qualify_as_needed() {
        let doc = "SELECT * FROM lo";
        let c = complete(doc, doc.len(), Dialect::Postgres, &catalog()).expect("items");
        assert_eq!(c.replace, 14..16);
        assert_eq!(c.items[0].insert, "audit.log");
        let doc = "SELECT u. FROM users u";
        let c = complete(doc, 9, Dialect::Postgres, &catalog()).expect("items");
        let name = c.items.iter().find(|i| i.label == "Name").expect("Name");
        assert_eq!(name.insert, "\"Name\"");
        assert_eq!(c.replace, 9..9);
    }

    #[test]
    fn mentions_cover_from_lists_joins_and_dml() {
        let m = mentions_at(
            "UPDATE s.t AS x SET a = 1; SELECT * FROM a, \"B\" b LEFT JOIN c ON true",
            40,
            Dialect::Postgres,
        );
        let names: Vec<(&str, Option<&str>)> = m
            .iter()
            .map(|m| (m.name.as_str(), m.alias.as_deref()))
            .collect();
        assert_eq!(names, vec![("a", None), ("B", Some("b")), ("c", None)]);
        let m = mentions_at("INSERT INTO s.t(a) VALUES (1)", 0, Dialect::Postgres);
        assert_eq!(m[0].schema.as_deref(), Some("s"));
        assert_eq!(m[0].name, "t");
    }
}

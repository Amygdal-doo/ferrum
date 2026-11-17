//! SQL formatter (the `sqlformat` crate): indentation, keyword case and
//! dialect. It reformats whitespace only and does not validate the SQL.

use sqlformat::{Dialect, FormatOptions, Indent, QueryParams};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeywordCase {
    Upper,
    Lower,
    /// As written.
    Keep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SqlDialect {
    Generic,
    Postgres,
    SqlServer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SqlOptions {
    /// Spaces per level; 0 for a tab.
    pub indent: u8,
    pub case: KeywordCase,
    pub dialect: SqlDialect,
}

impl Default for SqlOptions {
    fn default() -> Self {
        Self {
            indent: 2,
            case: KeywordCase::Upper,
            dialect: SqlDialect::Generic,
        }
    }
}

/// `sql` laid out one clause per line.
#[must_use]
pub fn format(sql: &str, options: SqlOptions) -> String {
    let options = FormatOptions {
        indent: match options.indent {
            0 => Indent::Tabs,
            n => Indent::Spaces(n),
        },
        uppercase: match options.case {
            KeywordCase::Upper => Some(true),
            KeywordCase::Lower => Some(false),
            KeywordCase::Keep => None,
        },
        lines_between_queries: 2,
        dialect: match options.dialect {
            SqlDialect::Generic => Dialect::Generic,
            SqlDialect::Postgres => Dialect::PostgreSql,
            SqlDialect::SqlServer => Dialect::SQLServer,
        },
        ..FormatOptions::default()
    };
    sqlformat::format(sql.trim(), &QueryParams::None, &options)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_a_query() {
        let out = format(
            "select id, name from users u join orders o on o.user_id = u.id where u.active = true order by name",
            SqlOptions::default(),
        );
        let lines: Vec<&str> = out.lines().collect();
        assert_eq!(lines[0], "SELECT");
        assert!(lines.contains(&"  id,"), "{out}");
        assert!(lines.contains(&"FROM"), "{out}");
        assert!(out.contains("WHERE"), "{out}");
        assert!(out.contains("ORDER BY"), "{out}");
    }

    #[test]
    fn keyword_case_and_indent() {
        let lower = format(
            "SELECT a FROM t",
            SqlOptions {
                case: KeywordCase::Lower,
                indent: 4,
                ..SqlOptions::default()
            },
        );
        assert_eq!(lower, "select\n    a\nfrom\n    t");
        let keep = format(
            "Select a From t",
            SqlOptions {
                case: KeywordCase::Keep,
                indent: 0,
                ..SqlOptions::default()
            },
        );
        assert_eq!(keep, "Select\n\ta\nFrom\n\tt");
    }

    #[test]
    fn several_statements_and_strings() {
        let out = format("select 'a;b'; select 2", SqlOptions::default());
        assert!(out.contains("'a;b'"), "{out}");
        assert_eq!(out.matches("SELECT").count(), 2, "{out}");
    }
}

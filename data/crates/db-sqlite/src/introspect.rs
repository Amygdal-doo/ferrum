//! Catalog queries against `sqlite_schema` and the `pragma_*` table functions.

use crate::error::map_sqlx_error;
use ferrumdata_db_driver_api::DbResult;
use ferrumdata_domain::{ColumnInfo, IndexInfo, TableDetails, TableInfo, TableKind, TableRef};
use sqlx::{Row, SqlitePool};

/// One row per (object, primary-key column); objects without a key appear once
/// with a NULL column. Internal `sqlite_%` tables are hidden.
const LIST_TABLES: &str = r"
SELECT m.name AS name, m.type AS kind, p.name AS pk_col
FROM sqlite_schema m
LEFT JOIN pragma_table_info(m.name) p ON p.pk > 0
WHERE m.type IN ('table', 'view')
  AND m.name NOT LIKE 'sqlite\_%' ESCAPE '\'
ORDER BY m.name, p.pk
";

pub async fn list_tables(pool: &SqlitePool) -> DbResult<Vec<TableInfo>> {
    let rows = sqlx::query(LIST_TABLES)
        .fetch_all(pool)
        .await
        .map_err(|e| map_sqlx_error(&e))?;
    let mut out: Vec<TableInfo> = Vec::new();
    for r in &rows {
        let name: String = r.try_get("name").map_err(|e| map_sqlx_error(&e))?;
        let kind: String = r.try_get("kind").map_err(|e| map_sqlx_error(&e))?;
        let pk: Option<String> = r.try_get("pk_col").map_err(|e| map_sqlx_error(&e))?;
        let same = out.last().is_some_and(|t| t.table.name == name);
        if !same {
            out.push(TableInfo {
                table: TableRef { schema: None, name },
                kind: if kind == "view" {
                    TableKind::View
                } else {
                    TableKind::Table
                },
                primary_key: Vec::new(),
                estimated_rows: None,
            });
        }
        if let (Some(col), Some(last)) = (pk, out.last_mut()) {
            last.primary_key.push(col);
        }
    }
    Ok(out)
}

const COLUMNS: &str = r#"
SELECT name, type, "notnull" AS not_null, dflt_value, pk
FROM pragma_table_info(?1)
ORDER BY cid
"#;

const INDEXES: &str = r#"
SELECT l.name AS index_name, l."unique" AS is_unique, l.origin AS origin, i.name AS col
FROM pragma_index_list(?1) l
LEFT JOIN pragma_index_info(l.name) i
ORDER BY l.name, i.seqno
"#;

/// Columns and indexes. A rowid alias (`INTEGER PRIMARY KEY`) has no index
/// of its own, so its key is listed as a synthetic `PRIMARY KEY` entry.
pub async fn describe_table(pool: &SqlitePool, table: &TableRef) -> DbResult<TableDetails> {
    let map = |e: sqlx::Error| map_sqlx_error(&e);
    let mut keyed: Vec<(i64, String)> = Vec::new();
    let mut columns = Vec::new();
    for r in sqlx::query(COLUMNS)
        .bind(&table.name)
        .fetch_all(pool)
        .await
        .map_err(map)?
    {
        let name: String = r.try_get("name").map_err(map)?;
        let data_type: String = r.try_get("type").map_err(map)?;
        let pk: i64 = r.try_get("pk").map_err(map)?;
        let not_null: i64 = r.try_get("not_null").map_err(map)?;
        if pk > 0 {
            keyed.push((pk, name.clone()));
        }
        columns.push(ColumnInfo {
            name,
            data_type: if data_type.is_empty() {
                "(no type)".into()
            } else {
                data_type
            },
            // A primary key column may hold NULL in SQLite unless declared NOT NULL,
            // except for the rowid alias; report what was declared.
            nullable: not_null == 0,
            default: r.try_get("dflt_value").map_err(map)?,
            primary_key: pk > 0,
        });
    }

    let mut indexes: Vec<IndexInfo> = Vec::new();
    for r in sqlx::query(INDEXES)
        .bind(&table.name)
        .fetch_all(pool)
        .await
        .map_err(map)?
    {
        let name: String = r.try_get("index_name").map_err(map)?;
        let col: Option<String> = r.try_get("col").map_err(map)?;
        let col = col.unwrap_or_else(|| "<expression>".into());
        if let Some(last) = indexes.last_mut().filter(|i| i.name == name) {
            last.columns.push(col);
            continue;
        }
        let unique: i64 = r.try_get("is_unique").map_err(map)?;
        let origin: String = r.try_get("origin").map_err(map)?;
        indexes.push(IndexInfo {
            name,
            columns: vec![col],
            unique: unique != 0,
            primary: origin == "pk",
        });
    }
    if !keyed.is_empty() && !indexes.iter().any(|i| i.primary) {
        keyed.sort();
        indexes.push(IndexInfo {
            name: "PRIMARY KEY".into(),
            columns: keyed.into_iter().map(|(_, c)| c).collect(),
            unique: true,
            primary: true,
        });
    }
    indexes.sort_by(|a, b| b.primary.cmp(&a.primary).then_with(|| a.name.cmp(&b.name)));
    Ok(TableDetails { columns, indexes })
}

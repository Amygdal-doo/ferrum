//! Catalog queries against `sqlite_schema` and `pragma_table_info`.

use crate::error::map_sqlx_error;
use ferrumdata_db_driver_api::DbResult;
use ferrumdata_domain::{TableInfo, TableKind, TableRef};
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

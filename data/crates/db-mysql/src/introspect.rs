//! Catalog queries against `information_schema`.
//!
//! Scoped to the profile's database when one is set (`DATABASE()`), otherwise
//! every non-system schema. Names are cast to CHAR because some servers report
//! catalog columns as binary strings.

use crate::error::map_sqlx_error;
use ferrumdata_db_driver_api::DbResult;
use ferrumdata_domain::{Secret, TableInfo, TableKind, TableRef};
use sqlx::{MySqlPool, Row};
use std::collections::HashMap;

const SYSTEM_SCHEMAS: &str = "('mysql', 'information_schema', 'performance_schema', 'sys')";

pub async fn list_tables(pool: &MySqlPool, secret: &Secret) -> DbResult<Vec<TableInfo>> {
    let map = |e: sqlx::Error| map_sqlx_error(&e, secret);
    let tables_sql = format!(
        "SELECT CAST(TABLE_SCHEMA AS CHAR) AS sch, CAST(TABLE_NAME AS CHAR) AS name,
                CAST(TABLE_TYPE AS CHAR) AS kind, CAST(TABLE_ROWS AS SIGNED) AS estimate
         FROM information_schema.TABLES
         WHERE TABLE_SCHEMA NOT IN {SYSTEM_SCHEMAS}
           AND (DATABASE() IS NULL OR TABLE_SCHEMA = DATABASE())"
    );
    let pk_sql = format!(
        "SELECT CAST(TABLE_SCHEMA AS CHAR) AS sch, CAST(TABLE_NAME AS CHAR) AS name,
                CAST(COLUMN_NAME AS CHAR) AS col
         FROM information_schema.KEY_COLUMN_USAGE
         WHERE CONSTRAINT_NAME = 'PRIMARY'
           AND TABLE_SCHEMA NOT IN {SYSTEM_SCHEMAS}
           AND (DATABASE() IS NULL OR TABLE_SCHEMA = DATABASE())
         ORDER BY TABLE_SCHEMA, TABLE_NAME, ORDINAL_POSITION"
    );
    let mut keys: HashMap<(String, String), Vec<String>> = HashMap::new();
    for r in sqlx::query(sqlx::AssertSqlSafe(pk_sql))
        .fetch_all(pool)
        .await
        .map_err(map)?
    {
        let sch: String = r.try_get("sch").map_err(map)?;
        let name: String = r.try_get("name").map_err(map)?;
        keys.entry((sch, name))
            .or_default()
            .push(r.try_get("col").map_err(map)?);
    }

    let mut out = Vec::new();
    for r in sqlx::query(sqlx::AssertSqlSafe(tables_sql))
        .fetch_all(pool)
        .await
        .map_err(map)?
    {
        let sch: String = r.try_get("sch").map_err(map)?;
        let name: String = r.try_get("name").map_err(map)?;
        let kind: String = r.try_get("kind").map_err(map)?;
        let estimate: Option<i64> = r.try_get("estimate").map_err(map)?;
        let primary_key = keys
            .remove(&(sch.clone(), name.clone()))
            .unwrap_or_default();
        out.push(TableInfo {
            table: TableRef {
                schema: Some(sch),
                name,
            },
            kind: if kind.contains("VIEW") {
                TableKind::View
            } else {
                TableKind::Table
            },
            primary_key,
            // InnoDB's TABLE_ROWS is an estimate; NULL for views.
            estimated_rows: estimate.filter(|n| *n >= 0),
        });
    }
    // Binary order, like the other drivers (MySQL's collation order is case-insensitive).
    out.sort_by(|a, b| {
        (a.table.schema.as_deref(), a.table.name.as_str())
            .cmp(&(b.table.schema.as_deref(), b.table.name.as_str()))
    });
    Ok(out)
}

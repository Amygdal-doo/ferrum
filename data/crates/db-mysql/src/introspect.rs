//! Catalog queries against `information_schema`.
//!
//! Scoped to the profile's database when one is set (`DATABASE()`), otherwise
//! every non-system schema. Names are cast to CHAR because some servers report
//! catalog columns as binary strings.

use crate::error::map_sqlx_error;
use ferrumdata_db_driver_api::DbResult;
use ferrumdata_domain::{
    ColumnInfo, IndexInfo, Secret, TableDetails, TableInfo, TableKind, TableRef,
};
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

const COLUMNS: &str = "
SELECT CAST(COLUMN_NAME AS CHAR) AS name, CAST(COLUMN_TYPE AS CHAR) AS data_type,
       CAST(IS_NULLABLE AS CHAR) AS nullable, CAST(COLUMN_DEFAULT AS CHAR) AS dflt,
       CAST(COLUMN_KEY AS CHAR) AS ckey, CAST(EXTRA AS CHAR) AS extra
FROM information_schema.COLUMNS
WHERE TABLE_SCHEMA = COALESCE(?, DATABASE()) AND TABLE_NAME = ?
ORDER BY ORDINAL_POSITION";

/// `COLUMN_NAME` is NULL for a functional key part (MySQL 8.0.13+); the
/// `EXPRESSION` column is not read because MariaDB lacks it.
const INDEXES: &str = "
SELECT CAST(INDEX_NAME AS CHAR) AS name, CAST(COLUMN_NAME AS CHAR) AS col,
       CAST(NON_UNIQUE AS SIGNED) AS non_unique
FROM information_schema.STATISTICS
WHERE TABLE_SCHEMA = COALESCE(?, DATABASE()) AND TABLE_NAME = ?
ORDER BY INDEX_NAME = 'PRIMARY' DESC, INDEX_NAME, SEQ_IN_INDEX";

pub async fn describe_table(
    pool: &MySqlPool,
    secret: &Secret,
    table: &TableRef,
) -> DbResult<TableDetails> {
    let map = |e: sqlx::Error| map_sqlx_error(&e, secret);
    let mut columns = Vec::new();
    for r in sqlx::query(COLUMNS)
        .bind(table.schema.as_deref())
        .bind(&table.name)
        .fetch_all(pool)
        .await
        .map_err(map)?
    {
        let data_type: String = r.try_get("data_type").map_err(map)?;
        let extra: Option<String> = r.try_get("extra").map_err(map)?;
        let nullable: String = r.try_get("nullable").map_err(map)?;
        let key: Option<String> = r.try_get("ckey").map_err(map)?;
        // `auto_increment`, `on update CURRENT_TIMESTAMP`, `VIRTUAL GENERATED`…
        // (`DEFAULT_GENERATED` only marks an expression default).
        let extra = extra
            .map(|e| e.replace("DEFAULT_GENERATED", "").trim().to_owned())
            .filter(|e| !e.is_empty());
        columns.push(ColumnInfo {
            name: r.try_get("name").map_err(map)?,
            data_type: match extra {
                Some(e) => format!("{data_type} {e}"),
                None => data_type,
            },
            nullable: nullable == "YES",
            default: r.try_get("dflt").map_err(map)?,
            primary_key: key.as_deref() == Some("PRI"),
        });
    }

    let mut indexes: Vec<IndexInfo> = Vec::new();
    for r in sqlx::query(INDEXES)
        .bind(table.schema.as_deref())
        .bind(&table.name)
        .fetch_all(pool)
        .await
        .map_err(map)?
    {
        let name: String = r.try_get("name").map_err(map)?;
        let col: Option<String> = r.try_get("col").map_err(map)?;
        let col = col.unwrap_or_else(|| "<expression>".into());
        if let Some(last) = indexes.last_mut().filter(|i| i.name == name) {
            last.columns.push(col);
            continue;
        }
        let non_unique: i64 = r.try_get("non_unique").map_err(map)?;
        indexes.push(IndexInfo {
            primary: name == "PRIMARY",
            name,
            columns: vec![col],
            unique: non_unique == 0,
        });
    }
    Ok(TableDetails { columns, indexes })
}

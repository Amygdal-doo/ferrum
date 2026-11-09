//! Catalog queries (pg_catalog, not information_schema, for speed and relkind detail).

use crate::error::map_sqlx_error;
use ferrumdata_db_driver_api::DbResult;
use ferrumdata_domain::{
    ColumnInfo, IndexInfo, Secret, TableDetails, TableInfo, TableKind, TableRef,
};
use sqlx::{PgPool, Row};

const LIST_TABLES: &str = r"
SELECT n.nspname::text AS schema,
       c.relname::text AS name,
       c.relkind::text AS kind,
       c.reltuples::int8 AS estimate,
       COALESCE((
           SELECT array_agg(a.attname::text ORDER BY k.ord)
           FROM pg_index i
           CROSS JOIN LATERAL unnest(i.indkey) WITH ORDINALITY AS k(attnum, ord)
           JOIN pg_attribute a ON a.attrelid = i.indrelid AND a.attnum = k.attnum
           WHERE i.indrelid = c.oid AND i.indisprimary
       ), ARRAY[]::text[]) AS pk
FROM pg_class c
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE c.relkind IN ('r', 'v', 'm', 'f', 'p')
  AND NOT c.relispartition
  AND n.nspname NOT IN ('pg_catalog', 'information_schema')
  AND n.nspname NOT LIKE 'pg\_toast%'
  AND n.nspname NOT LIKE 'pg\_temp\_%'
ORDER BY n.nspname, c.relname
";

pub async fn list_tables(pool: &PgPool, secret: &Secret) -> DbResult<Vec<TableInfo>> {
    let rows = sqlx::query(LIST_TABLES)
        .fetch_all(pool)
        .await
        .map_err(|e| map_sqlx_error(&e, secret))?;
    rows.iter()
        .map(|r| {
            let kind: String = r.try_get("kind")?;
            let estimate: i64 = r.try_get("estimate")?;
            Ok(TableInfo {
                table: TableRef {
                    schema: Some(r.try_get("schema")?),
                    name: r.try_get("name")?,
                },
                kind: relkind(&kind),
                primary_key: r.try_get("pk")?,
                // reltuples is -1 (PG 14+) or 0 before the first ANALYZE.
                estimated_rows: (estimate >= 0).then_some(estimate),
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(|e| map_sqlx_error(&e, secret))
}

fn relkind(k: &str) -> TableKind {
    match k {
        "v" => TableKind::View,
        "m" => TableKind::MaterializedView,
        "f" => TableKind::ForeignTable,
        "p" => TableKind::PartitionedTable,
        _ => TableKind::Table,
    }
}

const COLUMNS: &str = r"
SELECT a.attname::text AS name,
       format_type(a.atttypid, a.atttypmod) AS data_type,
       NOT a.attnotnull AS nullable,
       pg_get_expr(d.adbin, d.adrelid) AS default_expr,
       COALESCE(a.attnum = ANY (pk.indkey::int2[]), false) AS is_pk
FROM pg_attribute a
JOIN pg_class c ON c.oid = a.attrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
LEFT JOIN pg_attrdef d ON d.adrelid = a.attrelid AND d.adnum = a.attnum
LEFT JOIN pg_index pk ON pk.indrelid = c.oid AND pk.indisprimary
WHERE n.nspname = $1 AND c.relname = $2 AND a.attnum > 0 AND NOT a.attisdropped
ORDER BY a.attnum
";

/// Key columns only (INCLUDE columns are left out); expressions as written.
const INDEXES: &str = r"
SELECT ic.relname::text AS name,
       i.indisunique AS is_unique,
       i.indisprimary AS is_primary,
       ARRAY(
           SELECT pg_get_indexdef(i.indexrelid, k, true)
           FROM generate_series(1, i.indnkeyatts::int) AS k
           ORDER BY k
       ) AS columns
FROM pg_index i
JOIN pg_class ic ON ic.oid = i.indexrelid
JOIN pg_class c ON c.oid = i.indrelid
JOIN pg_namespace n ON n.oid = c.relnamespace
WHERE n.nspname = $1 AND c.relname = $2
ORDER BY i.indisprimary DESC, ic.relname
";

pub async fn describe_table(
    pool: &PgPool,
    secret: &Secret,
    table: &TableRef,
) -> DbResult<TableDetails> {
    let map = |e: sqlx::Error| map_sqlx_error(&e, secret);
    let schema = table.schema.as_deref().unwrap_or("public");
    let columns = sqlx::query(COLUMNS)
        .bind(schema)
        .bind(&table.name)
        .fetch_all(pool)
        .await
        .map_err(map)?
        .iter()
        .map(|r| {
            Ok(ColumnInfo {
                name: r.try_get("name")?,
                data_type: r.try_get("data_type")?,
                nullable: r.try_get("nullable")?,
                default: r.try_get("default_expr")?,
                primary_key: r.try_get("is_pk")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(map)?;
    let indexes = sqlx::query(INDEXES)
        .bind(schema)
        .bind(&table.name)
        .fetch_all(pool)
        .await
        .map_err(map)?
        .iter()
        .map(|r| {
            Ok(IndexInfo {
                name: r.try_get("name")?,
                columns: r.try_get("columns")?,
                unique: r.try_get("is_unique")?,
                primary: r.try_get("is_primary")?,
            })
        })
        .collect::<Result<_, sqlx::Error>>()
        .map_err(map)?;
    Ok(TableDetails { columns, indexes })
}

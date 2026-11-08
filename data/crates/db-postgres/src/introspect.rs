//! Catalog queries (pg_catalog, not information_schema, for speed and relkind detail).

use crate::error::map_sqlx_error;
use ferrumdata_db_driver_api::DbResult;
use ferrumdata_domain::{Secret, TableInfo, TableKind, TableRef};
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

//! SQL dump of chosen tables: schema (tables, sequences, indexes, foreign
//! keys) and data as INSERT statements, in the source engine's dialect,
//! streamed to a file so large tables never sit in memory.

use crate::QueryService;
use crate::ddl::{
    add_foreign_key, create_indexes, create_table, dependency_order, primary_key, sequences,
    table_name,
};
use ferrumdata_domain::{
    CellValue, ColumnMeta, DatabaseEngine, DbError, DbErrorKind, ExecutionId, ProfileId,
    QueryEvent, QueryLimits, QueryRequest, TableSchema,
};
use std::path::Path;
use tokio::io::{AsyncWriteExt, BufWriter};

/// Rows per INSERT statement.
const ROWS_PER_INSERT: usize = 100;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DumpOptions {
    pub schema: bool,
    pub data: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DumpSummary {
    pub tables: usize,
    pub rows: u64,
}

fn io(e: std::io::Error) -> DbError {
    DbError::new(
        DbErrorKind::Internal,
        format!("Writing the dump failed: {e}"),
    )
}

/// Writes the dump of `tables` to `path`. On failure the partial file is removed.
pub async fn write_dump(
    queries: &QueryService,
    engine: DatabaseEngine,
    profile_id: ProfileId,
    tables: &[TableSchema],
    options: DumpOptions,
    path: &Path,
) -> Result<DumpSummary, DbError> {
    let file = tokio::fs::File::create(path).await.map_err(io)?;
    let mut out = BufWriter::new(file);
    let result = write_all(queries, engine, profile_id, tables, options, &mut out).await;
    let result = match result {
        Ok(summary) => out.flush().await.map_err(io).map(|()| summary),
        Err(e) => Err(e),
    };
    if result.is_err()
        && let Err(e) = tokio::fs::remove_file(path).await
    {
        tracing::debug!(error = %e, "could not remove a partial dump");
    }
    result
}

async fn write_all(
    queries: &QueryService,
    engine: DatabaseEngine,
    profile_id: ProfileId,
    tables: &[TableSchema],
    options: DumpOptions,
    out: &mut BufWriter<tokio::fs::File>,
) -> Result<DumpSummary, DbError> {
    let refs: Vec<&TableSchema> = tables.iter().collect();
    let ordered: Vec<&TableSchema> = dependency_order(&refs)
        .into_iter()
        .map(|i| refs[i])
        .collect();
    let names: Vec<String> = ordered.iter().map(|t| t.info.table.name.clone()).collect();
    let mut head = format!(
        "-- FerrumData SQL dump ({}), {}\n-- Tables: {}\n\n",
        engine.label(),
        chrono::Utc::now().format("%Y-%m-%d %H:%M UTC"),
        names.join(", ")
    );
    match engine {
        DatabaseEngine::MySql => {
            head.push_str("SET FOREIGN_KEY_CHECKS = 0;\nSTART TRANSACTION;\n\n")
        }
        _ => head.push_str("BEGIN;\n\n"),
    }
    if options.schema && engine == DatabaseEngine::Postgres {
        let mut schemas: Vec<&str> = ordered
            .iter()
            .filter_map(|t| t.info.table.schema.as_deref())
            .filter(|s| *s != "public")
            .collect();
        schemas.dedup();
        for schema in schemas {
            head.push_str(&format!(
                "CREATE SCHEMA IF NOT EXISTS {};\n",
                engine.quote_ident(schema)
            ));
        }
    }
    out.write_all(head.as_bytes()).await.map_err(io)?;

    if options.schema {
        for table in &ordered {
            let mut ddl = String::new();
            for sequence in sequences(engine, table) {
                ddl.push_str(&format!("{sequence}\n"));
            }
            // SQLite cannot add foreign keys later; it accepts forward references.
            let inline = engine == DatabaseEngine::Sqlite;
            ddl.push_str(&format!("{}\n", create_table(engine, table, inline)));
            for index in create_indexes(engine, table) {
                ddl.push_str(&format!("{index}\n"));
            }
            ddl.push('\n');
            out.write_all(ddl.as_bytes()).await.map_err(io)?;
        }
    }

    let mut summary = DumpSummary {
        tables: ordered.len(),
        rows: 0,
    };
    if options.data {
        for table in &ordered {
            summary.rows += dump_rows(queries, engine, profile_id, table, out).await?;
        }
    }

    let mut tail = String::new();
    if options.schema && engine != DatabaseEngine::Sqlite {
        for table in &ordered {
            for fk in &table.details.foreign_keys {
                if ordered.iter().any(|t| t.info.table == fk.ref_table) {
                    tail.push_str(&format!(
                        "{}\n",
                        add_foreign_key(engine, &table.info.table, fk)
                    ));
                } else {
                    tail.push_str(&format!(
                        "-- {}: foreign key to {} (not in this dump) left out\n",
                        table.info.table.name, fk.ref_table.name
                    ));
                }
            }
        }
    }
    tail.push_str(match engine {
        DatabaseEngine::MySql => "\nCOMMIT;\nSET FOREIGN_KEY_CHECKS = 1;\n",
        _ => "\nCOMMIT;\n",
    });
    out.write_all(tail.as_bytes()).await.map_err(io)?;
    Ok(summary)
}

/// Streams one table's rows as multi-row INSERTs; returns the row count.
async fn dump_rows(
    queries: &QueryService,
    engine: DatabaseEngine,
    profile_id: ProfileId,
    table: &TableSchema,
    out: &mut BufWriter<tokio::fs::File>,
) -> Result<u64, DbError> {
    let mut sql = format!("SELECT * FROM {}", table.info.table.qualified(engine));
    let key = primary_key(table);
    if !key.is_empty() {
        let cols: Vec<String> = key.iter().map(|c| engine.quote_ident(c)).collect();
        sql.push_str(&format!(" ORDER BY {}", cols.join(", ")));
    }
    let request = QueryRequest {
        execution_id: ExecutionId::new(),
        profile_id,
        editor_id: None,
        sql,
        limits: QueryLimits {
            max_rows: usize::MAX,
            batch_rows: 500,
            timeout: None,
        },
    };
    let name = table_name(engine, &table.info.table);
    let mut events = queries.execute(request).await?;
    let mut columns: Vec<ColumnMeta> = Vec::new();
    let mut pending: Vec<String> = Vec::new();
    let mut count = 0u64;
    let mut serial_columns = Vec::new();
    while let Some(envelope) = events.recv().await {
        match envelope.event {
            QueryEvent::Metadata(meta) => columns = meta,
            QueryEvent::Rows(rows) => {
                for row in rows {
                    pending.push(values(engine, &table.info.table.name, &columns, &row)?);
                    count += 1;
                    if pending.len() >= ROWS_PER_INSERT {
                        let text = insert(&name, engine, &columns, &mut pending);
                        out.write_all(text.as_bytes()).await.map_err(io)?;
                    }
                }
            }
            QueryEvent::Failed(e) => return Err(e),
            QueryEvent::Cancelled { .. } | QueryEvent::TimedOut { .. } => {
                return Err(DbError::new(
                    DbErrorKind::Cancelled,
                    format!("Reading {} stopped", table.info.table.name),
                ));
            }
            _ => {}
        }
    }
    if !pending.is_empty() {
        let text = insert(&name, engine, &columns, &mut pending);
        out.write_all(text.as_bytes()).await.map_err(io)?;
    }
    // Inserted ids do not advance a PostgreSQL serial's sequence.
    for c in &table.details.columns {
        if let Some(seq) = c
            .default
            .as_deref()
            .and_then(|d| d.strip_prefix("nextval('"))
            .and_then(|d| d.split('\'').next())
            .filter(|_| count > 0 && engine == DatabaseEngine::Postgres)
        {
            serial_columns.push(format!(
                "SELECT setval('{seq}', (SELECT max({}) FROM {name}));\n",
                engine.quote_ident(&c.name)
            ));
        }
    }
    let mut tail = serial_columns.concat();
    tail.push('\n');
    out.write_all(tail.as_bytes()).await.map_err(io)?;
    Ok(count)
}

fn values(
    engine: DatabaseEngine,
    table: &str,
    columns: &[ColumnMeta],
    row: &[CellValue],
) -> Result<String, DbError> {
    let literals: Option<Vec<String>> = row.iter().map(|v| engine.literal(v)).collect();
    match literals {
        Some(l) => Ok(format!("({})", l.join(", "))),
        None => {
            let column = row
                .iter()
                .position(|v| engine.literal(v).is_none())
                .and_then(|i| columns.get(i))
                .map_or("?", |c| c.name.as_str());
            Err(DbError::new(
                DbErrorKind::Unsupported,
                format!("{table}.{column} holds values FerrumData cannot write back yet"),
            ))
        }
    }
}

fn insert(
    name: &str,
    engine: DatabaseEngine,
    columns: &[ColumnMeta],
    rows: &mut Vec<String>,
) -> String {
    let cols: Vec<String> = columns
        .iter()
        .map(|c| engine.quote_ident(&c.name))
        .collect();
    let text = format!(
        "INSERT INTO {name} ({}) VALUES\n  {};\n",
        cols.join(", "),
        rows.join(",\n  ")
    );
    rows.clear();
    text
}

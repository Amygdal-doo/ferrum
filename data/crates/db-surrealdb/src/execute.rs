//! Execution of a SurrealQL script with cancellation and a row limit.
//!
//! Rows are read from the SDK's item stream. Documents are buffered (at most
//! `max_rows`) so the grid's columns can be the union of their fields, then
//! sent in batches. A script with several statements shows the last one that
//! returned rows; the others are summarised in messages.

use crate::decode::document;
use crate::error::map_surreal_error;
use crate::guard;
use ferrumdata_db_driver_api::CancellationHandle;
use ferrumdata_db_driver_api::documents::{DocumentTable, send_table};
use ferrumdata_domain::{DbError, DbErrorKind, QueryEvent, QueryOutcome, QueryRequest, Secret};
use futures::StreamExt;
use std::time::{Duration, Instant};
use surrealdb::Surreal;
use surrealdb::engine::remote::ws::Client;
use surrealdb::method::StreamItem;
use surrealdb::types::Value;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct ExecContext {
    pub db: Surreal<Client>,
    pub secret: Secret,
    pub read_only: bool,
    pub cancel: CancellationHandle,
    pub shutdown: CancellationToken,
}

enum Ending {
    Finished(Result<Streamed, DbError>),
    Cancelled,
    TimedOut,
}

struct Streamed {
    rows_returned: usize,
    truncated: bool,
}

pub async fn run(ctx: ExecContext, request: QueryRequest, tx: mpsc::Sender<QueryEvent>) {
    let started = Instant::now();
    if tx.send(QueryEvent::Started).await.is_err() {
        return;
    }
    let terminal = execute(&ctx, &request, &tx, started).await;
    if tx.send(terminal).await.is_err() {
        tracing::debug!("query receiver dropped before the terminal event");
    }
}

async fn execute(
    ctx: &ExecContext,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
    started: Instant,
) -> QueryEvent {
    if ctx.read_only && guard::writes(&request.sql) {
        return QueryEvent::Failed(DbError::new(
            DbErrorKind::Query,
            "This profile is read-only: statements that write or change the schema are refused",
        ));
    }
    let timeout = request.limits.timeout.unwrap_or(Duration::MAX);
    // Dropping the stream (on cancel, timeout or receiver loss) stops the
    // execution on the SDK side.
    let ending = tokio::select! {
        r = tokio::time::timeout(timeout, stream(ctx, request, tx)) => match r {
            Ok(result) => Ending::Finished(result),
            Err(_) => Ending::TimedOut,
        },
        () = ctx.cancel.cancelled() => Ending::Cancelled,
        () = ctx.shutdown.cancelled() => Ending::Cancelled,
    };
    let elapsed = started.elapsed();
    match ending {
        Ending::Finished(Ok(s)) => QueryEvent::Completed(QueryOutcome {
            rows_returned: s.rows_returned,
            rows_affected: None,
            truncated: s.truncated,
            elapsed,
        }),
        Ending::Finished(Err(e)) => QueryEvent::Failed(e),
        Ending::Cancelled => QueryEvent::Cancelled { elapsed },
        Ending::TimedOut => QueryEvent::TimedOut { elapsed },
    }
}

async fn stream(
    ctx: &ExecContext,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
) -> Result<Streamed, DbError> {
    let map = |e: surrealdb::Error| map_surreal_error(&e, &ctx.secret, false);
    let max_rows = request.limits.max_rows;
    let mut items = ctx
        .db
        .query(request.sql.as_str())
        .stream_items()
        .map_err(map)?;

    // (statement index, rows) of the statement being read and of the last
    // finished statement that returned rows.
    let mut current: (usize, DocumentTable) = (0, DocumentTable::new());
    let mut shown: Option<(usize, DocumentTable)> = None;
    let mut statements = 0usize;
    let mut truncated = false;
    while let Some(item) = items.next().await {
        match item.map_err(map)? {
            StreamItem::Row { statement, value } => {
                if statement != current.0 {
                    current = (statement, DocumentTable::new());
                }
                // NONE is what DEFINE/REMOVE/LET… return: no row to show.
                if matches!(value, Value::None) {
                    continue;
                }
                if current.1.len() >= max_rows {
                    truncated = true;
                    break;
                }
                current.1.push(document(value));
            }
            StreamItem::StatementEnd {
                statement, result, ..
            } => {
                // An error retracts the statement's rows: report it, show nothing.
                result.map_err(|e| {
                    let mut err = map(e);
                    if statement > 0 {
                        err.detail = Some(format!("in statement {}", statement + 1));
                    }
                    err
                })?;
                statements = statements.max(statement + 1);
                if statement == current.0 {
                    let finished = std::mem::take(&mut current.1);
                    if !finished.is_empty() {
                        shown = Some((statement, finished));
                    }
                }
            }
        }
    }
    drop(items);
    if truncated {
        shown = Some(std::mem::take(&mut current));
    }

    let Some((index, table)) = shown else {
        return Ok(Streamed {
            rows_returned: 0,
            truncated: false,
        });
    };
    if statements > 1
        && tx
            .send(QueryEvent::Message(format!(
                "Ran {statements} statements; showing the result of statement {}.",
                index + 1
            )))
            .await
            .is_err()
    {
        return Ok(Streamed {
            rows_returned: 0,
            truncated,
        });
    }
    let (columns, rows) = table.finish(&["id"]);
    let rows_returned = rows.len();
    // A gone receiver means nobody is listening for the outcome either.
    let delivered = send_table(tx, columns, rows, request.limits.batch_rows).await;
    Ok(Streamed {
        rows_returned: if delivered { rows_returned } else { 0 },
        truncated,
    })
}

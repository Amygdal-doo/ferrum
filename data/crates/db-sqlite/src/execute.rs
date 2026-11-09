//! Streaming execution of one statement with cancellation and backpressure.

use crate::decode::{column_meta, decode_row};
use crate::error::map_sqlx_error;
use ferrumdata_db_driver_api::CancellationHandle;
use ferrumdata_domain::{DbError, QueryEvent, QueryOutcome, QueryRequest};
use futures::TryStreamExt;
use sqlx::pool::PoolConnection;
use sqlx::{AssertSqlSafe, Connection, Executor, SqlSafeStr, Sqlite, SqlitePool, Statement};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// VM instructions between progress-handler checks (~sub-millisecond).
const PROGRESS_OPS: i32 = 1_000;

pub struct ExecContext {
    pub pool: SqlitePool,
    pub cancel: CancellationHandle,
    pub shutdown: CancellationToken,
}

/// How streaming ended, decided before touching the connection again.
enum Ending {
    Finished(Result<Streamed, DbError>),
    Cancelled,
    TimedOut,
}

struct Streamed {
    rows_returned: usize,
    rows_affected: Option<u64>,
    truncated: bool,
    /// The statement may still be stepping on the worker thread.
    abandoned: bool,
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
    let mut conn = tokio::select! {
        c = ctx.pool.acquire() => match c {
            Ok(c) => c,
            Err(e) => return QueryEvent::Failed(map_sqlx_error(&e)),
        },
        () = ctx.cancel.cancelled() => return QueryEvent::Cancelled { elapsed: started.elapsed() },
    };

    // SQLite's progress handler is the engine-side cancel: once `interrupt` is
    // set, the running step fails with SQLITE_INTERRUPT, even mid-aggregate.
    let interrupt = CancellationToken::new();
    match conn.lock_handle().await {
        Ok(mut handle) => {
            let flag = interrupt.clone();
            handle.set_progress_handler(PROGRESS_OPS, move || !flag.is_cancelled());
        }
        Err(e) => return QueryEvent::Failed(map_sqlx_error(&e)),
    }

    let timeout = request.limits.timeout.unwrap_or(Duration::MAX);
    let ending = tokio::select! {
        r = tokio::time::timeout(timeout, stream(&mut conn, request, tx)) => match r {
            Ok(result) => Ending::Finished(result),
            Err(_) => Ending::TimedOut,
        },
        () = ctx.cancel.cancelled() => Ending::Cancelled,
        () = ctx.shutdown.cancelled() => Ending::Cancelled,
    };

    let elapsed = started.elapsed();
    let abandoned = match &ending {
        Ending::Finished(Ok(s)) => s.abandoned,
        Ending::Finished(Err(_)) => false,
        Ending::Cancelled | Ending::TimedOut => true,
    };
    if abandoned {
        // Stop the statement, then close the connection in the background
        // rather than returning a possibly-busy one to the pool, so the
        // cancel is acknowledged immediately.
        interrupt.cancel();
        tokio::spawn(async move {
            if let Err(e) = conn.detach().close().await {
                tracing::debug!(error = %e, "closing abandoned sqlite connection failed");
            }
        });
    } else {
        // The handler captures this run's token; clear it before reuse.
        let reset = match conn.lock_handle().await {
            Ok(mut handle) => {
                handle.remove_progress_handler();
                Ok(())
            }
            Err(e) => Err(e),
        };
        if let Err(e) = reset {
            tracing::warn!(error = %e, "could not reset sqlite progress handler");
            tokio::spawn(async move {
                if let Err(e) = conn.detach().close().await {
                    tracing::debug!(error = %e, "closing sqlite connection failed");
                }
            });
        }
    }
    match ending {
        Ending::Finished(Ok(s)) => QueryEvent::Completed(QueryOutcome {
            rows_returned: s.rows_returned,
            rows_affected: s.rows_affected,
            truncated: s.truncated,
            elapsed,
        }),
        Ending::Finished(Err(e)) => QueryEvent::Failed(e),
        Ending::Cancelled => QueryEvent::Cancelled { elapsed },
        Ending::TimedOut => QueryEvent::TimedOut { elapsed },
    }
}

async fn stream(
    conn: &mut PoolConnection<Sqlite>,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
) -> Result<Streamed, DbError> {
    let map = |e: sqlx::Error| map_sqlx_error(&e);
    // Raw SQL typed by the user is executed as-is, by design: FerrumData is a SQL
    // client. Protection comes from read-only opening, not parsing.
    let sql = AssertSqlSafe(request.sql.clone()).into_sql_str();
    let statement = (&mut **conn).prepare(sql).await.map_err(map)?;
    let columns = column_meta(statement.columns());

    if columns.is_empty() {
        let result = statement.query().execute(&mut **conn).await.map_err(map)?;
        return Ok(Streamed {
            rows_returned: 0,
            rows_affected: Some(result.rows_affected()),
            truncated: false,
            abandoned: false,
        });
    }

    let receiver_gone = || Streamed {
        rows_returned: 0,
        rows_affected: None,
        truncated: false,
        abandoned: true,
    };
    let declared_bool: Vec<bool> = columns.iter().map(|c| c.db_type == "BOOLEAN").collect();
    if tx.send(QueryEvent::Metadata(columns)).await.is_err() {
        return Ok(receiver_gone());
    }

    let batch_size = request.limits.batch_rows.max(1);
    let max_rows = request.limits.max_rows;
    let mut rows = statement.query().fetch(&mut **conn);
    let mut batch = Vec::with_capacity(batch_size);
    let mut total = 0usize;
    let mut truncated = false;

    while let Some(row) = rows.try_next().await.map_err(map)? {
        if total >= max_rows {
            truncated = true;
            break;
        }
        batch.push(decode_row(&row, &declared_bool));
        total += 1;
        if batch.len() >= batch_size {
            // Backpressure point: no more rows are stepped until the
            // consumer frees a slot.
            if tx
                .send(QueryEvent::Rows(std::mem::take(&mut batch)))
                .await
                .is_err()
            {
                return Ok(receiver_gone());
            }
            batch.reserve(batch_size);
        }
    }
    if !batch.is_empty() && tx.send(QueryEvent::Rows(batch)).await.is_err() {
        return Ok(receiver_gone());
    }
    Ok(Streamed {
        rows_returned: total,
        rows_affected: None,
        truncated,
        abandoned: truncated,
    })
}

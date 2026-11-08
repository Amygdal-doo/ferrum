//! Streaming execution of one statement with cancellation and backpressure.

use crate::decode::{column_meta, decode_row};
use crate::error::map_sqlx_error;
use crate::options::PgOptions;
use ferrumdata_db_driver_api::CancellationHandle;
use ferrumdata_domain::{DbError, QueryEvent, QueryOutcome, QueryRequest, Secret};
use futures::TryStreamExt;
use sqlx::pool::PoolConnection;
use sqlx::postgres::PgConnection;
use sqlx::{AssertSqlSafe, Connection, Executor, PgPool, Postgres, SqlSafeStr, Statement};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct ExecContext {
    pub pool: PgPool,
    pub options: PgOptions,
    pub secret: Secret,
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
    /// The server may still be producing rows for this session.
    abandoned: bool,
}

pub async fn run(ctx: ExecContext, request: QueryRequest, tx: mpsc::Sender<QueryEvent>) {
    let started = Instant::now();
    if tx.send(QueryEvent::Started).await.is_err() {
        return;
    }
    let terminal = execute(&ctx, &request, &tx, started).await;
    // Receiver may be gone; nothing else to do in that case.
    let _ = tx.send(terminal).await;
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
            Err(e) => return QueryEvent::Failed(map_sqlx_error(&e, &ctx.secret)),
        },
        () = ctx.cancel.cancelled() => return QueryEvent::Cancelled { elapsed: started.elapsed() },
    };
    let pid: i32 = match sqlx::query_scalar("SELECT pg_backend_pid()")
        .fetch_one(&mut *conn)
        .await
    {
        Ok(pid) => pid,
        Err(e) => return QueryEvent::Failed(map_sqlx_error(&e, &ctx.secret)),
    };

    let timeout = request.limits.timeout.unwrap_or(Duration::MAX);
    let ending = tokio::select! {
        r = tokio::time::timeout(timeout, stream(&mut conn, request, tx, &ctx.secret)) => match r {
            Ok(result) => Ending::Finished(result),
            Err(_) => Ending::TimedOut,
        },
        () = ctx.cancel.cancelled() => Ending::Cancelled,
        () = ctx.shutdown.cancelled() => Ending::Cancelled,
    };

    let elapsed = started.elapsed();
    let needs_server_cancel = match &ending {
        Ending::Finished(Ok(s)) => s.abandoned,
        Ending::Finished(Err(_)) => false,
        Ending::Cancelled | Ending::TimedOut => true,
    };
    if needs_server_cancel {
        taint(conn, &ctx.options, pid).await;
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
    conn: &mut PoolConnection<Postgres>,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
    secret: &Secret,
) -> Result<Streamed, DbError> {
    let map = |e: sqlx::Error| map_sqlx_error(&e, secret);
    // Raw SQL typed by the user is executed as-is, by design: FerrumData is a SQL
    // client. Safety is enforced by DB roles/read-only sessions, not parsing.
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
        batch.push(decode_row(&row));
        total += 1;
        if batch.len() >= batch_size {
            // Awaiting here is the backpressure point: no more rows are
            // pulled from the socket until the consumer frees a slot.
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

/// Asks the server to stop the backend's current query, then closes the
/// session rather than returning a possibly-busy connection to the pool.
async fn taint(conn: PoolConnection<Postgres>, options: &PgOptions, pid: i32) {
    match PgConnection::connect_with(&options.0).await {
        Ok(mut side) => {
            if let Err(e) = sqlx::query("SELECT pg_cancel_backend($1)")
                .bind(pid)
                .execute(&mut side)
                .await
            {
                tracing::warn!(error = %e, pid, "pg_cancel_backend failed");
            }
            let _ = side.close().await;
        }
        Err(e) => tracing::warn!(error = %e, "could not open side connection to cancel query"),
    }
    let _ = conn.detach().close().await;
}

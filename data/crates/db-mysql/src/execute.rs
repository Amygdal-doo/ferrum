//! Streaming execution of one statement with cancellation and backpressure.

use crate::decode::{column_meta, decode_row};
use crate::error::map_sqlx_error;
use crate::options::MySqlOptions;
use ferrumdata_db_driver_api::CancellationHandle;
use ferrumdata_domain::{DbError, QueryEvent, QueryOutcome, QueryRequest, Secret};
use futures::TryStreamExt;
use sqlx::mysql::{MySqlConnection, MySqlDatabaseError, MySqlQueryResult, MySqlRow};
use sqlx::pool::PoolConnection;
use sqlx::{
    AssertSqlSafe, Connection, Either, Executor, MySql, MySqlPool, Row, SqlSafeStr, Statement,
};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// ER_UNSUPPORTED_PS: the statement cannot be prepared (e.g. `LOCK TABLES`).
const ER_UNSUPPORTED_PS: u16 = 1295;

pub struct ExecContext {
    pub pool: MySqlPool,
    pub options: MySqlOptions,
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
            Err(e) => return QueryEvent::Failed(map_sqlx_error(&e, &ctx.secret)),
        },
        () = ctx.cancel.cancelled() => return QueryEvent::Cancelled { elapsed: started.elapsed() },
    };
    let thread_id: u64 = match sqlx::query_scalar("SELECT CONNECTION_ID()")
        .fetch_one(&mut *conn)
        .await
    {
        Ok(id) => id,
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
        taint(conn, &ctx.options, thread_id).await;
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
    conn: &mut PoolConnection<MySql>,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
    secret: &Secret,
) -> Result<Streamed, DbError> {
    let map = |e: sqlx::Error| map_sqlx_error(&e, secret);
    // Raw SQL typed by the user is executed as-is, by design: FerrumData is a SQL
    // client. Safety is enforced by DB users/read-only sessions, not parsing.
    let sql = AssertSqlSafe(request.sql.clone()).into_sql_str();
    let statement = match (&mut **conn).prepare(sql).await {
        Ok(s) => s,
        Err(e) if is_unsupported_ps(&e) => {
            // Text protocol fallback for statements MySQL cannot prepare.
            let sql = AssertSqlSafe(request.sql.clone()).into_sql_str();
            let results = sqlx::raw_sql(sql).fetch_many(&mut **conn);
            return stream_late(results, request, tx, secret).await;
        }
        Err(e) => return Err(map(e)),
    };
    let columns = column_meta(statement.columns());

    if columns.is_empty() {
        // No columns when prepared does not mean no rows: EXPLAIN reports
        // its column only once executed. Run it as text, which reports both
        // rows and the affected-row count.
        drop(statement);
        let sql = AssertSqlSafe(request.sql.clone()).into_sql_str();
        let results = sqlx::raw_sql(sql).fetch_many(&mut **conn);
        return stream_late(results, request, tx, secret).await;
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

/// Streams a statement whose columns are known only from its first row
/// (`EXPLAIN …`, text-protocol fallback). Without rows it reports the
/// affected-row count like any other DML.
async fn stream_late<S>(
    mut results: S,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
    secret: &Secret,
) -> Result<Streamed, DbError>
where
    S: futures::Stream<Item = Result<Either<MySqlQueryResult, MySqlRow>, sqlx::Error>> + Unpin,
{
    let map = |e: sqlx::Error| map_sqlx_error(&e, secret);
    let batch_size = request.limits.batch_rows.max(1);
    let mut batch = Vec::with_capacity(batch_size);
    let mut affected = 0u64;
    let mut total = 0usize;
    let mut has_columns = false;
    let gone = Streamed {
        rows_returned: 0,
        rows_affected: None,
        truncated: false,
        abandoned: true,
    };
    while let Some(item) = results.try_next().await.map_err(map)? {
        let row = match item {
            Either::Left(done) => {
                affected += done.rows_affected();
                continue;
            }
            Either::Right(row) => row,
        };
        if !has_columns {
            has_columns = true;
            if tx
                .send(QueryEvent::Metadata(column_meta(row.columns())))
                .await
                .is_err()
            {
                return Ok(gone);
            }
        }
        if total >= request.limits.max_rows {
            return Ok(Streamed {
                rows_returned: total,
                rows_affected: None,
                truncated: true,
                abandoned: true,
            });
        }
        batch.push(decode_row(&row));
        total += 1;
        if batch.len() >= batch_size
            && tx
                .send(QueryEvent::Rows(std::mem::take(&mut batch)))
                .await
                .is_err()
        {
            return Ok(gone);
        }
    }
    if !batch.is_empty() && tx.send(QueryEvent::Rows(batch)).await.is_err() {
        return Ok(gone);
    }
    Ok(Streamed {
        rows_returned: total,
        rows_affected: (!has_columns).then_some(affected),
        truncated: false,
        abandoned: false,
    })
}

fn is_unsupported_ps(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|db| db.try_downcast_ref::<MySqlDatabaseError>())
        .is_some_and(|my| my.number() == ER_UNSUPPORTED_PS)
}

/// Asks the server to stop the session's current statement, then closes the
/// session rather than returning a possibly-busy connection to the pool.
async fn taint(conn: PoolConnection<MySql>, options: &MySqlOptions, thread_id: u64) {
    match MySqlConnection::connect_with(&options.0).await {
        Ok(mut side) => {
            // KILL does not accept placeholders; the id is a server-issued integer.
            let kill = AssertSqlSafe(format!("KILL QUERY {thread_id}"));
            if let Err(e) = sqlx::raw_sql(kill).execute(&mut side).await {
                tracing::warn!(error = %e, thread_id, "KILL QUERY failed");
            }
            if let Err(e) = side.close().await {
                tracing::debug!(error = %e, "closing side connection failed");
            }
        }
        Err(e) => tracing::warn!(error = %e, "could not open side connection to cancel query"),
    }
    if let Err(e) = conn.detach().close().await {
        tracing::debug!(error = %e, "closing abandoned mysql connection failed");
    }
}

//! Console execution of one command, with cancellation, timeout and the
//! read-only allow-list.
//!
//! Each run opens its own connection, so a blocking command (`BLPOP 0`,
//! `XREAD BLOCK …`) never stalls the key browser. Cancel and timeout drop
//! that connection; Redis then abandons the command (client-side cancel).

use crate::command::{self, CommandClass};
use crate::error::map_redis_error;
use crate::reply::reply_table;
use ferrumdata_db_driver_api::CancellationHandle;
use ferrumdata_db_driver_api::documents::send_table;
use ferrumdata_domain::{DbError, DbErrorKind, QueryEvent, QueryOutcome, QueryRequest, Secret};
use redis::{AsyncConnectionConfig, Client, Value};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct ExecContext {
    pub client: Client,
    pub secret: Secret,
    pub read_only: bool,
    pub connect_timeout: Duration,
    pub cancel: CancellationHandle,
    /// Cancelled when the connection is closed.
    pub shutdown: CancellationToken,
}

/// Checks a console command before it runs. Returns the parsed arguments.
pub fn check(text: &str, read_only: bool) -> Result<Vec<Vec<u8>>, DbError> {
    let args = command::split_args(text.trim()).map_err(|e| {
        let mut err = DbError::new(DbErrorKind::Query, format!("Invalid command: {e}"));
        err.hint = Some("Quote arguments with spaces: SET greeting \"hello world\"".into());
        err
    })?;
    match command::classify(&args) {
        CommandClass::Refused(why) => Err(DbError::unsupported(why)),
        CommandClass::Write if read_only => {
            let mut err = DbError::new(
                DbErrorKind::Query,
                format!(
                    "This profile is read-only: {} is not on the read-only command allow-list",
                    command::command_name(&args)
                ),
            );
            err.hint = Some(
                "Reads such as GET, HGETALL, SCAN, TYPE, TTL and INFO are allowed. \
                 Turn off read-only in the connection profile to write."
                    .into(),
            );
            Err(err)
        }
        CommandClass::Read | CommandClass::Write => Ok(args),
    }
}

pub async fn run(ctx: ExecContext, request: QueryRequest, tx: mpsc::Sender<QueryEvent>) {
    let started = Instant::now();
    if tx.send(QueryEvent::Started).await.is_err() {
        return;
    }
    let terminal = execute(ctx, &request, &tx, started).await;
    if tx.send(terminal).await.is_err() {
        tracing::debug!("query receiver dropped before the terminal event");
    }
}

enum Ending {
    Finished(Result<(usize, bool), DbError>),
    Cancelled,
    TimedOut,
}

async fn execute(
    ctx: ExecContext,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
    started: Instant,
) -> QueryEvent {
    let args = match check(&request.sql, ctx.read_only) {
        Ok(a) => a,
        Err(e) => return QueryEvent::Failed(e),
    };
    let work = async {
        let value = query(&ctx, &args).await?;
        let shape = command::reply_shape(&args);
        let table = reply_table(value, shape, request.limits.max_rows);
        let rows = table.rows.len();
        if !send_table(tx, table.columns, table.rows, request.limits.batch_rows).await {
            tracing::debug!("query receiver dropped");
        }
        Ok((rows, table.truncated))
    };
    let ending = tokio::select! {
        r = with_timeout(request.limits.timeout, work) => match r {
            Some(result) => Ending::Finished(result),
            None => Ending::TimedOut,
        },
        () = ctx.cancel.cancelled() => Ending::Cancelled,
        () = ctx.shutdown.cancelled() => Ending::Cancelled,
    };
    let elapsed = started.elapsed();
    match ending {
        Ending::Finished(Ok((rows_returned, truncated))) => QueryEvent::Completed(QueryOutcome {
            rows_returned,
            rows_affected: None,
            truncated,
            elapsed,
        }),
        Ending::Finished(Err(e)) if e.kind == DbErrorKind::Timeout => {
            QueryEvent::TimedOut { elapsed }
        }
        Ending::Finished(Err(e)) => QueryEvent::Failed(e),
        Ending::Cancelled => QueryEvent::Cancelled { elapsed },
        Ending::TimedOut => QueryEvent::TimedOut { elapsed },
    }
}

/// Opens a dedicated connection and runs the command on it. Our own timeout
/// and cancel apply, so the client's response timeout is off.
async fn query(ctx: &ExecContext, args: &[Vec<u8>]) -> Result<Value, DbError> {
    let map = |e: redis::RedisError| map_redis_error(&e, &ctx.secret);
    let config = AsyncConnectionConfig::new()
        .set_connection_timeout(Some(ctx.connect_timeout))
        .set_response_timeout(None);
    let mut conn = ctx
        .client
        .get_multiplexed_async_connection_with_config(&config)
        .await
        .map_err(map)?;
    let mut cmd = redis::cmd(&String::from_utf8_lossy(&args[0]));
    for arg in &args[1..] {
        cmd.arg(arg.as_slice());
    }
    cmd.query_async::<Value>(&mut conn).await.map_err(map)
}

async fn with_timeout<T>(timeout: Option<Duration>, f: impl Future<Output = T>) -> Option<T> {
    match timeout {
        Some(t) => tokio::time::timeout(t, f).await.ok(),
        None => Some(f.await),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn read_only_profiles_refuse_writes_with_the_command_name() {
        let err = check("del user:1", true).expect_err("refused");
        assert_eq!(err.kind, DbErrorKind::Query);
        assert!(err.message.contains("read-only"), "{}", err.message);
        assert!(err.message.contains("DEL"), "{}", err.message);
        assert!(check("GET user:1", true).is_ok());
        assert!(check("del user:1", false).is_ok());
    }

    #[test]
    fn refused_and_malformed_commands_fail_before_running() {
        assert_eq!(
            check("SUBSCRIBE news", false).expect_err("refused").kind,
            DbErrorKind::Unsupported
        );
        assert_eq!(
            check("GET \"x", false).expect_err("bad").kind,
            DbErrorKind::Query
        );
        assert!(check("   ", false).is_err());
    }
}

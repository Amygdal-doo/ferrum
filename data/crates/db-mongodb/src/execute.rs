//! Execution of one MongoDB query with cancellation, timeout and a row limit.
//!
//! Documents are buffered (at most `max_rows`) so the grid's columns can be
//! the union of their top-level fields, then sent in batches. Cancel, timeout
//! and truncation drop the cursor, which kills it on the server; `max_time`
//! bounds server-side work for each operation.

use crate::decode::{document, scalar};
use crate::error::map_mongo_error;
use crate::query::{self, MongoQuery};
use bson::{Bson, Document as BsonDocument, doc};
use ferrumdata_db_driver_api::CancellationHandle;
use ferrumdata_db_driver_api::documents::{DocumentTable, send_table};
use ferrumdata_domain::{DbError, DbErrorKind, QueryEvent, QueryOutcome, QueryRequest, Secret};
use futures::TryStreamExt;
use mongodb::{Cursor, Database};
use std::time::{Duration, Instant};
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

pub struct ExecContext {
    pub db: Database,
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
    rows_affected: Option<u64>,
    truncated: bool,
}

/// What an operation produced, before it is sent to the grid.
enum Output {
    Table {
        table: DocumentTable,
        truncated: bool,
        note: Option<String>,
    },
    Affected {
        count: u64,
        note: String,
    },
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
    let query = match query::parse(&request.sql) {
        Ok(q) => q,
        Err(e) => {
            let mut err = DbError::new(DbErrorKind::Query, format!("Invalid query: {e}"));
            err.hint = Some(
                "Use db.<collection>.find({…}) / aggregate([…]) / countDocuments({…}), \
                 or a JSON command like { \"ping\": 1 }."
                    .into(),
            );
            return QueryEvent::Failed(err);
        }
    };
    if ctx.read_only && query.is_write() {
        return QueryEvent::Failed(DbError::new(
            DbErrorKind::Query,
            "This profile is read-only: writes and schema changes are refused",
        ));
    }
    let ending = tokio::select! {
        r = with_timeout(request.limits.timeout, stream(ctx, query, request, tx)) => match r {
            Some(result) => Ending::Finished(result),
            None => Ending::TimedOut,
        },
        () = ctx.cancel.cancelled() => Ending::Cancelled,
        () = ctx.shutdown.cancelled() => Ending::Cancelled,
    };
    let elapsed = started.elapsed();
    match ending {
        Ending::Finished(Ok(s)) => QueryEvent::Completed(QueryOutcome {
            rows_returned: s.rows_returned,
            rows_affected: s.rows_affected,
            truncated: s.truncated,
            elapsed,
        }),
        // The server's own max_time limit is still a timeout to the user.
        Ending::Finished(Err(e)) if e.kind == DbErrorKind::Timeout => {
            QueryEvent::TimedOut { elapsed }
        }
        Ending::Finished(Err(e)) => QueryEvent::Failed(e),
        Ending::Cancelled => QueryEvent::Cancelled { elapsed },
        Ending::TimedOut => QueryEvent::TimedOut { elapsed },
    }
}

async fn with_timeout<T>(timeout: Option<Duration>, f: impl Future<Output = T>) -> Option<T> {
    match timeout {
        Some(t) => tokio::time::timeout(t, f).await.ok(),
        None => Some(f.await),
    }
}

async fn stream(
    ctx: &ExecContext,
    query: MongoQuery,
    request: &QueryRequest,
    tx: &mpsc::Sender<QueryEvent>,
) -> Result<Streamed, DbError> {
    let output = operate(ctx, query, request).await?;
    match output {
        Output::Affected { count, note } => {
            // Receiver loss only means nobody hears the outcome.
            if tx.send(QueryEvent::Message(note)).await.is_err() {
                tracing::debug!("query receiver dropped");
            }
            Ok(Streamed {
                rows_returned: 0,
                rows_affected: Some(count),
                truncated: false,
            })
        }
        Output::Table {
            table,
            truncated,
            note,
        } => {
            if let Some(note) = note
                && tx.send(QueryEvent::Message(note)).await.is_err()
            {
                tracing::debug!("query receiver dropped");
            }
            let (columns, rows) = table.finish(&["_id"]);
            let rows_returned = rows.len();
            if !columns.is_empty() {
                send_table(tx, columns, rows, request.limits.batch_rows).await;
            }
            Ok(Streamed {
                rows_returned,
                rows_affected: None,
                truncated,
            })
        }
    }
}

async fn operate(
    ctx: &ExecContext,
    query: MongoQuery,
    request: &QueryRequest,
) -> Result<Output, DbError> {
    let map = |e: mongodb::error::Error| map_mongo_error(&e, &ctx.secret);
    let max_rows = request.limits.max_rows;
    let batch = u32::try_from(request.limits.batch_rows.max(1)).unwrap_or(u32::MAX);
    let max_time = request.limits.timeout;
    let coll = |name: &str| ctx.db.collection::<BsonDocument>(name);
    let table_of = |docs: Vec<BsonDocument>| {
        let mut table = DocumentTable::new();
        for d in docs {
            table.push(document(d));
        }
        Output::Table {
            table,
            truncated: false,
            note: None,
        }
    };

    Ok(match query {
        MongoQuery::Find {
            collection,
            filter,
            projection,
            sort,
            limit,
            skip,
            one: false,
        } => {
            let c = coll(&collection);
            let find = c
                .find(filter)
                .batch_size(batch)
                .optional(projection, |f, p| f.projection(p))
                .optional(sort, |f, s| f.sort(s))
                .optional(limit, |f, l| f.limit(l))
                .optional(skip, |f, s| f.skip(s))
                .optional(max_time, |o, t| o.max_time(t));
            drain(find.await.map_err(map)?, max_rows, &map).await?
        }
        MongoQuery::Find {
            collection,
            filter,
            projection,
            one: true,
            ..
        } => {
            let c = coll(&collection);
            let find = c
                .find_one(filter)
                .optional(projection, |f, p| f.projection(p))
                .optional(max_time, |o, t| o.max_time(t));
            table_of(find.await.map_err(map)?.into_iter().collect())
        }
        MongoQuery::Aggregate {
            collection,
            pipeline,
        } => {
            let c = coll(&collection);
            let agg = c
                .aggregate(pipeline)
                .batch_size(batch)
                .optional(max_time, |o, t| o.max_time(t));
            drain(agg.await.map_err(map)?, max_rows, &map).await?
        }
        MongoQuery::Count { collection, filter } => {
            let c = coll(&collection);
            let count = c
                .count_documents(filter)
                .optional(max_time, |o, t| o.max_time(t));
            let n = count.await.map_err(map)?;
            table_of(vec![doc! { "count": count_bson(n) }])
        }
        MongoQuery::EstimatedCount { collection } => {
            let c = coll(&collection);
            let count = c
                .estimated_document_count()
                .optional(max_time, |o, t| o.max_time(t));
            let n = count.await.map_err(map)?;
            table_of(vec![doc! { "count": count_bson(n) }])
        }
        MongoQuery::Distinct {
            collection,
            field,
            filter,
        } => {
            let c = coll(&collection);
            let distinct = c
                .distinct(field, filter)
                .optional(max_time, |o, t| o.max_time(t));
            let values = distinct.await.map_err(map)?;
            let truncated = values.len() > max_rows;
            let mut table = DocumentTable::new();
            for v in values.into_iter().take(max_rows) {
                table.push(scalar(v));
            }
            Output::Table {
                table,
                truncated,
                note: None,
            }
        }
        MongoQuery::InsertOne {
            collection,
            document,
        } => {
            let r = coll(&collection).insert_one(document).await.map_err(map)?;
            Output::Affected {
                count: 1,
                note: format!("Inserted _id {}", r.inserted_id),
            }
        }
        MongoQuery::InsertMany {
            collection,
            documents,
        } => {
            let r = coll(&collection)
                .insert_many(documents)
                .await
                .map_err(map)?;
            let count = r.inserted_ids.len() as u64;
            Output::Affected {
                count,
                note: format!("Inserted {count} documents"),
            }
        }
        MongoQuery::Update {
            collection,
            filter,
            update,
            many,
        } => {
            let c = coll(&collection);
            let r = if many {
                c.update_many(filter, update).await
            } else {
                c.update_one(filter, update).await
            }
            .map_err(map)?;
            Output::Affected {
                count: r.modified_count,
                note: format!("Matched {}, modified {}", r.matched_count, r.modified_count),
            }
        }
        MongoQuery::Replace {
            collection,
            filter,
            replacement,
        } => {
            let r = coll(&collection)
                .replace_one(filter, replacement)
                .await
                .map_err(map)?;
            Output::Affected {
                count: r.modified_count,
                note: format!("Matched {}, modified {}", r.matched_count, r.modified_count),
            }
        }
        MongoQuery::Delete {
            collection,
            filter,
            many,
        } => {
            let c = coll(&collection);
            let r = if many {
                c.delete_many(filter).await
            } else {
                c.delete_one(filter).await
            }
            .map_err(map)?;
            Output::Affected {
                count: r.deleted_count,
                note: format!("Deleted {}", r.deleted_count),
            }
        }
        MongoQuery::Command(command) => {
            let reply = ctx.db.run_command(command).await.map_err(map)?;
            command_output(reply, max_rows)
        }
    })
}

/// Counts are shown as integers; `u64` beyond `i64` is unrealistic but kept exact.
fn count_bson(n: u64) -> Bson {
    i64::try_from(n).map_or_else(|_| Bson::String(n.to_string()), Bson::Int64)
}

/// Reads a cursor up to `max_rows` documents. Dropping the cursor afterwards
/// kills it on the server when more results remain.
async fn drain(
    mut cursor: Cursor<BsonDocument>,
    max_rows: usize,
    map: &impl Fn(mongodb::error::Error) -> DbError,
) -> Result<Output, DbError> {
    let mut table = DocumentTable::new();
    let mut truncated = false;
    while let Some(doc) = cursor.try_next().await.map_err(map)? {
        if table.len() >= max_rows {
            truncated = true;
            break;
        }
        table.push(document(doc));
    }
    Ok(Output::Table {
        table,
        truncated,
        note: None,
    })
}

/// A command reply: the documents of `cursor.firstBatch` when present (find,
/// aggregate, listCollections…), otherwise the reply itself as one row.
fn command_output(mut reply: BsonDocument, max_rows: usize) -> Output {
    let first_batch = match reply.get_document_mut("cursor") {
        Ok(cursor) => match cursor.remove("firstBatch") {
            Some(Bson::Array(items)) => Some((items, cursor.get_i64("id").unwrap_or(0) != 0)),
            _ => None,
        },
        Err(_) => None,
    };
    let mut table = DocumentTable::new();
    let Some((items, more_on_server)) = first_batch else {
        table.push(document(reply));
        return Output::Table {
            table,
            truncated: false,
            note: None,
        };
    };
    let truncated = items.len() > max_rows;
    for item in items.into_iter().take(max_rows) {
        table.push(match item {
            Bson::Document(d) => document(d),
            other => scalar(other),
        });
    }
    Output::Table {
        table,
        truncated: truncated || more_on_server,
        note: more_on_server.then(|| {
            "Showing the command's first batch only; use db.<collection>.find(…) to read all results."
                .to_owned()
        }),
    }
}

/// Applies a builder method only when the value is present.
trait Optional: Sized {
    fn optional<T>(self, value: Option<T>, f: impl FnOnce(Self, T) -> Self) -> Self {
        match value {
            Some(v) => f(self, v),
            None => self,
        }
    }
}

impl<T> Optional for T {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_replies_show_first_batch_or_the_reply() {
        let reply = doc! { "cursor": { "id": 0_i64, "ns": "db.c", "firstBatch": [ { "a": 1 }, { "a": 2 } ] }, "ok": 1.0 };
        match command_output(reply, 10) {
            Output::Table {
                table,
                truncated,
                note,
            } => {
                assert_eq!(table.len(), 2);
                assert!(!truncated);
                assert!(note.is_none());
            }
            Output::Affected { .. } => panic!("expected a table"),
        }
        let open = doc! { "cursor": { "id": 42_i64, "firstBatch": [ { "a": 1 } ] }, "ok": 1.0 };
        assert!(matches!(
            command_output(open, 10),
            Output::Table {
                truncated: true,
                note: Some(_),
                ..
            }
        ));
        match command_output(doc! { "n": 3, "ok": 1.0 }, 10) {
            Output::Table { table, .. } => assert_eq!(table.len(), 1),
            Output::Affected { .. } => panic!("expected a table"),
        }
    }

    #[test]
    fn first_batch_respects_max_rows() {
        let reply =
            doc! { "cursor": { "id": 0_i64, "firstBatch": [ {"a": 1}, {"a": 2}, {"a": 3} ] } };
        match command_output(reply, 2) {
            Output::Table {
                table, truncated, ..
            } => {
                assert_eq!(table.len(), 2);
                assert!(truncated);
            }
            Output::Affected { .. } => panic!("expected a table"),
        }
    }
}

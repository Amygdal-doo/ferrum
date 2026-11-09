//! Driver contract tests against real SQLite database files. These need no
//! server, so they always run (each test uses its own temporary file).

use ferrumdata_db_driver_api::{DatabaseDriver, DbConnection};
use ferrumdata_db_sqlite::SqliteDriver;
use ferrumdata_domain::{
    CellValue, ConnectionProfile, DatabaseEngine, DbErrorKind, Endpoint, ExecutionId, ProfileId,
    QueryEvent, QueryLimits, QueryRequest, ResolvedConnectionConfig, TableKind,
};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

/// Infinite row generator: only limits, cancel or timeout end it.
const ENDLESS: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT x FROM c";
/// Infinite single-step aggregate: only the progress-handler interrupt ends it.
const ENDLESS_AGGREGATE: &str =
    "WITH RECURSIVE c(x) AS (SELECT 1 UNION ALL SELECT x + 1 FROM c) SELECT count(*) FROM c";

fn config(path: &str, read_only: bool) -> ResolvedConnectionConfig {
    let mut profile = ConnectionProfile::new(DatabaseEngine::Sqlite, "it");
    profile.endpoint = Endpoint::File { path: path.into() };
    profile.read_only = read_only;
    ResolvedConnectionConfig {
        profile,
        password: None,
    }
}

/// An empty file is a valid empty SQLite database.
fn new_db() -> (TempDir, String) {
    let dir = tempfile::tempdir().expect("tmp dir");
    let path = dir.path().join("it.db");
    std::fs::write(&path, b"").expect("create db file");
    (dir, path.to_string_lossy().into_owned())
}

async fn connect(path: &str, read_only: bool) -> Arc<dyn DbConnection> {
    SqliteDriver
        .connect(&config(path, read_only))
        .await
        .expect("connect")
}

fn req(sql: &str, limits: QueryLimits) -> QueryRequest {
    QueryRequest {
        execution_id: ExecutionId::new(),
        profile_id: ProfileId::new(),
        editor_id: None,
        sql: sql.into(),
        limits,
    }
}

async fn run(conn: &Arc<dyn DbConnection>, sql: &str, limits: QueryLimits) -> Vec<QueryEvent> {
    let mut exec = conn.execute(req(sql, limits)).await.expect("execute");
    let mut out = Vec::new();
    while let Some(e) = exec.events.recv().await {
        out.push(e);
    }
    out
}

async fn ok(conn: &Arc<dyn DbConnection>, sql: &str) {
    let events = run(conn, sql, QueryLimits::default()).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::Completed(_))),
        "{sql}: {events:?}"
    );
}

fn rows(events: &[QueryEvent]) -> Vec<Vec<CellValue>> {
    events
        .iter()
        .filter_map(|e| match e {
            QueryEvent::Rows(r) => Some(r.clone()),
            _ => None,
        })
        .flatten()
        .collect()
}

#[tokio::test]
async fn connects_and_reports_version() {
    let (_dir, path) = new_db();
    let conn = connect(&path, false).await;
    let info = conn.server_info().await.expect("info");
    assert!(info.version.starts_with("SQLite 3."), "{info:?}");
    conn.close().await;
}

#[tokio::test]
async fn missing_file_is_connection_error() {
    let err = SqliteDriver
        .connect(&config("/no/such/dir/x.db", false))
        .await
        .err()
        .expect("must fail");
    assert_eq!(err.kind, DbErrorKind::Connection);
}

#[tokio::test]
async fn decodes_types_and_distinguishes_null_from_empty() {
    let (_dir, path) = new_db();
    let conn = connect(&path, false).await;
    ok(
        &conn,
        "create table t (n text, e text, u text, i integer, f real, b blob, flag boolean)",
    )
    .await;
    ok(
        &conn,
        "insert into t values (null, '', 'héllo 😀', 42, 1.5, x'dead', 1)",
    )
    .await;
    let events = run(&conn, "select * from t", QueryLimits::default()).await;
    let meta = events.iter().find_map(|e| match e {
        QueryEvent::Metadata(m) => Some(m.clone()),
        _ => None,
    });
    let meta = meta.expect("metadata");
    assert_eq!(meta[3].db_type, "INTEGER");
    assert_eq!(meta[6].db_type, "BOOLEAN");
    let row = &rows(&events)[0];
    assert_eq!(row[0], CellValue::Null);
    assert_eq!(row[1], CellValue::Text(String::new()));
    assert_eq!(row[2], CellValue::Text("héllo 😀".into()));
    assert_eq!(row[3], CellValue::Signed(42));
    assert_eq!(row[4], CellValue::Float(1.5));
    assert_eq!(row[5], CellValue::Binary(vec![0xde, 0xad]));
    assert_eq!(row[6], CellValue::Bool(true));
    // Expression columns are typed by their value.
    let expr = run(&conn, "select 1 + 1, 'x' || 'y'", QueryLimits::default()).await;
    assert_eq!(
        rows(&expr),
        vec![vec![CellValue::Signed(2), CellValue::Text("xy".into())]]
    );
    conn.close().await;
}

#[tokio::test]
async fn streams_in_batches_and_truncates_at_max_rows() {
    let (_dir, path) = new_db();
    let conn = connect(&path, false).await;
    let limits = QueryLimits {
        max_rows: 1_000,
        batch_rows: 100,
        timeout: None,
    };
    let events = run(&conn, ENDLESS, limits).await;
    let batches = events
        .iter()
        .filter(|e| matches!(e, QueryEvent::Rows(_)))
        .count();
    assert_eq!(batches, 10);
    match events.last() {
        Some(QueryEvent::Completed(o)) => {
            assert_eq!(o.rows_returned, 1_000);
            assert!(o.truncated);
        }
        other => panic!("unexpected terminal: {other:?}"),
    }
    let again = run(&conn, "select 1", QueryLimits::default()).await;
    assert_eq!(rows(&again), vec![vec![CellValue::Signed(1)]]);
    conn.close().await;
}

#[tokio::test]
async fn cancel_interrupts_a_long_step_and_pool_recovers() {
    let (_dir, path) = new_db();
    let conn = connect(&path, false).await;
    let mut exec = conn
        .execute(req(ENDLESS_AGGREGATE, QueryLimits::default()))
        .await
        .expect("exec");
    assert_eq!(exec.events.recv().await, Some(QueryEvent::Started));
    tokio::time::sleep(Duration::from_millis(300)).await;
    exec.cancellation.cancel();
    let terminal = tokio::time::timeout(Duration::from_secs(5), async {
        let mut last = None;
        while let Some(e) = exec.events.recv().await {
            last = Some(e);
        }
        last
    })
    .await
    .expect("prompt cancel");
    assert!(
        matches!(terminal, Some(QueryEvent::Cancelled { .. })),
        "{terminal:?}"
    );
    // Closing the pool would hang if the interrupted step were still running.
    let again = run(&conn, "select 1", QueryLimits::default()).await;
    assert_eq!(rows(&again), vec![vec![CellValue::Signed(1)]]);
    tokio::time::timeout(Duration::from_secs(5), conn.close())
        .await
        .expect("pool closes: no statement left running");
}

#[tokio::test]
async fn timeout_is_reported() {
    let (_dir, path) = new_db();
    let conn = connect(&path, false).await;
    let limits = QueryLimits {
        timeout: Some(Duration::from_millis(300)),
        ..QueryLimits::default()
    };
    let events = run(&conn, ENDLESS_AGGREGATE, limits).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::TimedOut { .. })),
        "{events:?}"
    );
    tokio::time::timeout(Duration::from_secs(5), conn.close())
        .await
        .expect("pool closes");
}

#[tokio::test]
async fn ddl_and_dml_report_affected_rows_and_errors_are_structured() {
    let (_dir, path) = new_db();
    let conn = connect(&path, false).await;
    ok(&conn, "create table t (id integer primary key)").await;
    let ins = run(
        &conn,
        "insert into t values (1), (2), (3)",
        QueryLimits::default(),
    )
    .await;
    assert!(matches!(ins.last(), Some(QueryEvent::Completed(o)) if o.rows_affected == Some(3)));
    let dup = run(&conn, "insert into t values (1)", QueryLimits::default()).await;
    match dup.last() {
        Some(QueryEvent::Failed(e)) => {
            assert_eq!(e.kind, DbErrorKind::Query);
            // SQLITE_CONSTRAINT_PRIMARYKEY
            assert_eq!(e.code.as_deref(), Some("1555"));
        }
        other => panic!("expected constraint violation, got {other:?}"),
    }
    let bad = run(&conn, "selec nonsense", QueryLimits::default()).await;
    assert!(matches!(bad.last(), Some(QueryEvent::Failed(e)) if e.kind == DbErrorKind::Query));
    conn.close().await;
}

#[tokio::test]
async fn read_only_profile_rejects_writes() {
    let (_dir, path) = new_db();
    let rw = connect(&path, false).await;
    ok(&rw, "create table t (id integer)").await;
    rw.close().await;
    let ro = connect(&path, true).await;
    let events = run(&ro, "insert into t values (1)", QueryLimits::default()).await;
    match events.last() {
        // SQLITE_READONLY (8)
        Some(QueryEvent::Failed(e)) => assert_eq!(e.code.as_deref(), Some("8"), "{e:?}"),
        other => panic!("expected read-only failure, got {other:?}"),
    }
    ro.close().await;
}

#[tokio::test]
async fn lists_tables_with_case_preserved_names_and_primary_keys() {
    let (_dir, path) = new_db();
    let conn = connect(&path, false).await;
    ok(
        &conn,
        r#"create table "User" ("orgId" int, id text, primary key ("orgId", id))"#,
    )
    .await;
    ok(&conn, "create table no_pk (x int)").await;
    ok(&conn, r#"create view v_users as select * from "User""#).await;
    // Creates the internal sqlite_sequence table, which must stay hidden.
    ok(
        &conn,
        "create table auto (id integer primary key autoincrement)",
    )
    .await;
    let tables = conn.list_tables().await.expect("list");
    let names: Vec<_> = tables.iter().map(|t| t.table.name.as_str()).collect();
    assert_eq!(names, vec!["User", "auto", "no_pk", "v_users"]);
    assert_eq!(
        tables[0].primary_key,
        vec!["orgId".to_owned(), "id".to_owned()]
    );
    assert!(tables[2].primary_key.is_empty());
    assert_eq!(tables[3].kind, TableKind::View);
    conn.close().await;
}

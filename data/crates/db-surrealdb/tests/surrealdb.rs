//! Driver contract tests against a real SurrealDB server.
//!
//! Skipped unless `FERRUMDATA_TEST_SURREAL=1`. Start the server with
//! `docker compose up -d surrealdb`. Override the target with
//! `FERRUMDATA_TEST_SURREAL_HOST`, `_PORT`, `_USER`, `_PASSWORD`.
//! Each test uses its own database inside the `ferrumdata` namespace.

use ferrumdata_db_driver_api::{DatabaseDriver, DbConnection};
use ferrumdata_db_surrealdb::SurrealDriver;
use ferrumdata_domain::{
    CellValue, ConnectionProfile, DatabaseEngine, DbErrorKind, Endpoint, ExecutionId, ProfileId,
    QueryEvent, QueryLimits, QueryRequest, ResolvedConnectionConfig, Secret, TableKind,
};
use std::sync::Arc;
use std::time::Duration;

fn env(key: &str, default: &str) -> String {
    std::env::var(format!("FERRUMDATA_TEST_SURREAL_{key}")).unwrap_or_else(|_| default.to_owned())
}

fn enabled() -> bool {
    let on = std::env::var("FERRUMDATA_TEST_SURREAL").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipping: set FERRUMDATA_TEST_SURREAL=1 to run SurrealDB integration tests");
    }
    on
}

fn config(password: &str, read_only: bool) -> ResolvedConnectionConfig {
    let mut profile = ConnectionProfile::new(DatabaseEngine::SurrealDb, "it");
    profile.endpoint = Endpoint::Tcp {
        host: env("HOST", "127.0.0.1"),
        port: env("PORT", "18000").parse().expect("port"),
    };
    profile.username = Some(env("USER", "root"));
    profile.namespace = Some("ferrumdata".into());
    profile.database = Some(format!("t_{}", ExecutionId::new().0.simple()));
    profile.read_only = read_only;
    ResolvedConnectionConfig {
        profile,
        password: Some(Secret::new(password)),
    }
}

fn good() -> ResolvedConnectionConfig {
    config(&env("PASSWORD", "ferrumdata-dev-password"), false)
}

async fn connect(config: &ResolvedConnectionConfig) -> Arc<dyn DbConnection> {
    SurrealDriver.connect(config).await.expect("connect")
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

async fn ok(conn: &Arc<dyn DbConnection>, sql: &str) -> Vec<QueryEvent> {
    let events = run(conn, sql, QueryLimits::default()).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::Completed(_))),
        "{sql}: {events:?}"
    );
    events
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

fn column_names(events: &[QueryEvent]) -> Vec<String> {
    events
        .iter()
        .find_map(|e| match e {
            QueryEvent::Metadata(m) => Some(m.iter().map(|c| c.name.clone()).collect()),
            _ => None,
        })
        .unwrap_or_default()
}

#[tokio::test]
async fn connects_and_reports_version() {
    if !enabled() {
        return;
    }
    let conn = connect(&good()).await;
    let info = conn.server_info().await.expect("info");
    assert!(info.version.starts_with("SurrealDB "), "{info:?}");
    conn.close().await;
}

#[tokio::test]
async fn bad_password_is_authentication_error_without_secret() {
    if !enabled() {
        return;
    }
    let err = SurrealDriver
        .connect(&config("wrong-pass-XYZ", false))
        .await
        .err()
        .expect("must fail");
    assert_eq!(err.kind, DbErrorKind::Authentication, "{err:?}");
    assert!(!format!("{err:?}").contains("wrong-pass-XYZ"));
}

#[tokio::test]
async fn flattens_documents_into_typed_columns() {
    if !enabled() {
        return;
    }
    let conn = connect(&good()).await;
    ok(
        &conn,
        "CREATE person:ann CONTENT { name: 'Ann', age: 41, score: 1.5, born: d'2024-01-02T03:04:05Z', \
         tags: ['a', 'b'], address: { city: 'Oslo' }, nothing: NULL, empty: '', price: 10.25dec }",
    )
    .await;
    ok(
        &conn,
        "CREATE person:bob CONTENT { name: 'Bob', nickname: 'B' }",
    )
    .await;
    let events = ok(&conn, "SELECT * FROM person ORDER BY id").await;
    let names = column_names(&events);
    assert_eq!(names[0], "id", "{names:?}");
    let col = |n: &str| names.iter().position(|c| c == n).expect(n);
    let all = rows(&events);
    let ann = &all[0];
    assert_eq!(ann[col("id")], CellValue::Text("person:ann".into()));
    assert_eq!(ann[col("name")], CellValue::Text("Ann".into()));
    assert_eq!(ann[col("age")], CellValue::Signed(41));
    assert_eq!(ann[col("score")], CellValue::Float(1.5));
    assert_eq!(ann[col("born")].display(), "2024-01-02T03:04:05+00:00");
    assert_eq!(ann[col("tags")], CellValue::Json("[\"a\",\"b\"]".into()));
    assert_eq!(
        ann[col("address")],
        CellValue::Json("{\"city\":\"Oslo\"}".into())
    );
    assert_eq!(ann[col("nothing")], CellValue::Null);
    assert_eq!(ann[col("empty")], CellValue::Text(String::new()));
    assert_eq!(ann[col("price")], CellValue::Decimal("10.25".into()));
    // Bob lacks Ann's fields and Ann lacks Bob's nickname: absent, not NULL.
    let bob = &all[1];
    assert_eq!(bob[col("age")], CellValue::Unsupported("absent".into()));
    assert_eq!(
        ann[col("nickname")],
        CellValue::Unsupported("absent".into())
    );
    // Scalars land in a single `value` column.
    let scalar = ok(&conn, "RETURN 1 + 1").await;
    assert_eq!(column_names(&scalar), ["value"]);
    assert_eq!(rows(&scalar), vec![vec![CellValue::Signed(2)]]);
    conn.close().await;
}

#[tokio::test]
async fn multi_statement_scripts_show_the_last_result_and_errors_are_structured() {
    if !enabled() {
        return;
    }
    let conn = connect(&good()).await;
    let events = ok(
        &conn,
        "DEFINE TABLE item SCHEMALESS; CREATE item:1 SET n = 1; SELECT n FROM item",
    )
    .await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, QueryEvent::Message(m) if m.contains("statement 3")))
    );
    assert_eq!(rows(&events), vec![vec![CellValue::Signed(1)]]);
    let bad = run(&conn, "SELEC nonsense", QueryLimits::default()).await;
    match bad.last() {
        Some(QueryEvent::Failed(e)) => assert_eq!(e.kind, DbErrorKind::Query, "{e:?}"),
        other => panic!("expected parse error, got {other:?}"),
    }
    let dup = run(&conn, "CREATE item:1 SET n = 2", QueryLimits::default()).await;
    assert!(
        matches!(dup.last(), Some(QueryEvent::Failed(e)) if e.kind == DbErrorKind::Query),
        "{dup:?}"
    );
    conn.close().await;
}

#[tokio::test]
async fn streams_in_batches_and_truncates_at_max_rows() {
    if !enabled() {
        return;
    }
    let conn = connect(&good()).await;
    let limits = QueryLimits {
        max_rows: 1_000,
        batch_rows: 100,
        timeout: None,
    };
    let events = run(&conn, "RETURN array::range(1, 5000)", limits).await;
    let batches = events
        .iter()
        .filter(|e| matches!(e, QueryEvent::Rows(_)))
        .count();
    assert_eq!(batches, 10, "{:?}", events.last());
    match events.last() {
        Some(QueryEvent::Completed(o)) => {
            assert_eq!(o.rows_returned, 1_000);
            assert!(o.truncated);
        }
        other => panic!("unexpected terminal: {other:?}"),
    }
    let again = ok(&conn, "RETURN 1").await;
    assert_eq!(rows(&again), vec![vec![CellValue::Signed(1)]]);
    conn.close().await;
}

#[tokio::test]
async fn cancel_and_timeout_are_prompt_and_the_connection_stays_usable() {
    if !enabled() {
        return;
    }
    let conn = connect(&good()).await;
    let mut exec = conn
        .execute(req("SLEEP 30s", QueryLimits::default()))
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
    let limits = QueryLimits {
        timeout: Some(Duration::from_millis(300)),
        ..QueryLimits::default()
    };
    let events = run(&conn, "SLEEP 10s", limits).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::TimedOut { .. })),
        "{events:?}"
    );
    let again = ok(&conn, "RETURN 'still here'").await;
    assert_eq!(
        rows(&again),
        vec![vec![CellValue::Text("still here".into())]]
    );
    conn.close().await;
}

#[tokio::test]
async fn read_only_profile_refuses_writes_client_side() {
    if !enabled() {
        return;
    }
    let mut cfg = good();
    cfg.profile.read_only = true;
    let conn = connect(&cfg).await;
    let events = run(&conn, "CREATE person:x", QueryLimits::default()).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::Failed(e)) if e.message.contains("read-only")),
        "{events:?}"
    );
    ok(&conn, "INFO FOR DB").await;
    conn.close().await;
}

#[tokio::test]
async fn lists_tables_and_views() {
    if !enabled() {
        return;
    }
    let conn = connect(&good()).await;
    ok(&conn, "CREATE User:1 SET orgId = 1").await;
    ok(&conn, "DEFINE TABLE no_pk SCHEMAFULL").await;
    ok(
        &conn,
        "DEFINE TABLE v_users AS SELECT count() AS n FROM User GROUP ALL",
    )
    .await;
    let tables = conn.list_tables().await.expect("list");
    let names: Vec<_> = tables.iter().map(|t| t.table.name.as_str()).collect();
    assert_eq!(names, vec!["User", "no_pk", "v_users"]);
    assert_eq!(tables[0].primary_key, vec!["id".to_owned()]);
    assert_eq!(tables[2].kind, TableKind::View);
    // The browse query generated for a table must run.
    let browse = ok(&conn, "SELECT * FROM `User` ORDER BY `id` LIMIT 11").await;
    assert_eq!(rows(&browse).len(), 1);
    conn.close().await;
}

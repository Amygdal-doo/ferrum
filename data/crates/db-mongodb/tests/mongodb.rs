//! Driver contract tests against a real MongoDB server.
//!
//! Skipped unless `FERRUMDATA_TEST_MONGO=1`. Start the server with
//! `docker compose up -d mongodb` (no auth). Override the target with
//! `FERRUMDATA_TEST_MONGO_URI`. Each test uses its own database.

use ferrumdata_db_driver_api::{DatabaseDriver, DbConnection};
use ferrumdata_db_mongodb::MongoDriver;
use ferrumdata_domain::{
    CellValue, ConnectionProfile, DatabaseEngine, DbErrorKind, Endpoint, ExecutionId, ProfileId,
    QueryEvent, QueryLimits, QueryRequest, ResolvedConnectionConfig, Secret, TableKind,
};
use std::sync::Arc;
use std::time::Duration;

fn enabled() -> bool {
    let on = std::env::var("FERRUMDATA_TEST_MONGO").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipping: set FERRUMDATA_TEST_MONGO=1 to run MongoDB integration tests");
    }
    on
}

fn config() -> ResolvedConnectionConfig {
    let mut profile = ConnectionProfile::new(DatabaseEngine::MongoDb, "it");
    profile.endpoint = Endpoint::Uri {
        uri: std::env::var("FERRUMDATA_TEST_MONGO_URI")
            .unwrap_or_else(|_| "mongodb://127.0.0.1:27019/?directConnection=true".into()),
    };
    profile.database = Some(format!("t_{}", ExecutionId::new().0.simple()));
    profile.connect_timeout_secs = 5;
    ResolvedConnectionConfig {
        profile,
        password: None,
    }
}

async fn connect(config: &ResolvedConnectionConfig) -> Arc<dyn DbConnection> {
    MongoDriver.connect(config).await.expect("connect")
}

fn req(text: &str, limits: QueryLimits) -> QueryRequest {
    QueryRequest {
        execution_id: ExecutionId::new(),
        profile_id: ProfileId::new(),
        editor_id: None,
        sql: text.into(),
        limits,
    }
}

async fn run(conn: &Arc<dyn DbConnection>, text: &str, limits: QueryLimits) -> Vec<QueryEvent> {
    let mut exec = conn.execute(req(text, limits)).await.expect("execute");
    let mut out = Vec::new();
    while let Some(e) = exec.events.recv().await {
        out.push(e);
    }
    out
}

async fn ok(conn: &Arc<dyn DbConnection>, text: &str) -> Vec<QueryEvent> {
    let events = run(conn, text, QueryLimits::default()).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::Completed(_))),
        "{text}: {events:?}"
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

fn affected(events: &[QueryEvent]) -> Option<u64> {
    match events.last() {
        Some(QueryEvent::Completed(o)) => o.rows_affected,
        _ => None,
    }
}

async fn drop_db(conn: &Arc<dyn DbConnection>) {
    ok(conn, r#"{ "dropDatabase": 1 }"#).await;
    conn.close().await;
}

#[tokio::test]
async fn connects_and_reports_version() {
    if !enabled() {
        return;
    }
    let conn = connect(&config()).await;
    let info = conn.server_info().await.expect("info");
    assert!(info.version.starts_with("MongoDB "), "{info:?}");
    conn.close().await;
}

#[tokio::test]
async fn unreachable_server_is_a_connection_error() {
    if !enabled() {
        return;
    }
    let mut cfg = config();
    cfg.profile.endpoint = Endpoint::Uri {
        uri: "mongodb://127.0.0.1:1/?directConnection=true".into(),
    };
    cfg.profile.connect_timeout_secs = 1;
    let err = MongoDriver.connect(&cfg).await.err().expect("must fail");
    assert_eq!(err.kind, DbErrorKind::Connection, "{err:?}");
}

#[tokio::test]
async fn bad_credentials_are_authentication_errors_without_secret() {
    if !enabled() {
        return;
    }
    let mut cfg = config();
    cfg.profile.username = Some("nobody".into());
    cfg.password = Some(Secret::new("wrong-pass-XYZ"));
    let err = MongoDriver.connect(&cfg).await.err().expect("must fail");
    assert_eq!(err.kind, DbErrorKind::Authentication, "{err:?}");
    assert!(!format!("{err:?}").contains("wrong-pass-XYZ"));
}

#[tokio::test]
async fn writes_then_finds_with_flattened_typed_columns() {
    if !enabled() {
        return;
    }
    let conn = connect(&config()).await;
    let ins = ok(
        &conn,
        "db.people.insertMany([\
           { _id: 1, name: 'Ann', age: 41, score: 1.5, born: ISODate('2024-01-02T03:04:05Z'), \
             tags: ['a', 'b'], address: { city: 'Oslo' }, nothing: null, empty: '', \
             price: NumberDecimal('10.25'), big: NumberLong('9007199254740993') },\
           { _id: 2, name: 'Bob', nickname: 'B' },\
         ])",
    )
    .await;
    assert_eq!(affected(&ins), Some(2));
    let events = ok(&conn, "db.people.find({}).sort({ _id: 1 })").await;
    let names = column_names(&events);
    assert_eq!(names[0], "_id");
    let col = |n: &str| names.iter().position(|c| c == n).expect(n);
    let all = rows(&events);
    let ann = &all[0];
    assert_eq!(ann[col("_id")], CellValue::Signed(1));
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
    assert_eq!(ann[col("big")], CellValue::Signed(9_007_199_254_740_993));
    assert_eq!(all[1][col("age")], CellValue::Unsupported("absent".into()));
    assert_eq!(
        ann[col("nickname")],
        CellValue::Unsupported("absent".into())
    );

    let one = ok(
        &conn,
        "db.people.findOne({ name: 'Bob' }, { _id: 0, nickname: 1 })",
    )
    .await;
    assert_eq!(column_names(&one), ["nickname"]);
    assert_eq!(rows(&one), vec![vec![CellValue::Text("B".into())]]);

    let upd = ok(
        &conn,
        "db.people.updateMany({ age: { $exists: false } }, { $set: { age: 7 } })",
    )
    .await;
    assert_eq!(affected(&upd), Some(1));
    let count = ok(&conn, "db.people.countDocuments({ age: { $gte: 7 } })").await;
    assert_eq!(rows(&count), vec![vec![CellValue::Signed(2)]]);
    let distinct = ok(&conn, "db.people.distinct('name')").await;
    assert_eq!(column_names(&distinct), ["value"]);
    assert_eq!(rows(&distinct).len(), 2);
    let agg = ok(
        &conn,
        "db.people.aggregate([{ $group: { _id: null, total: { $sum: '$age' } } }])",
    )
    .await;
    assert_eq!(
        rows(&agg),
        vec![vec![CellValue::Null, CellValue::Signed(48)]]
    );
    let cmd = ok(&conn, r#"{ "count": "people" }"#).await;
    let cmd_cols = column_names(&cmd);
    assert_eq!(
        rows(&cmd)[0][cmd_cols.iter().position(|c| c == "n").expect("n")],
        CellValue::Signed(2)
    );
    let del = ok(&conn, "db.people.deleteOne({ _id: 2 })").await;
    assert_eq!(affected(&del), Some(1));
    drop_db(&conn).await;
}

#[tokio::test]
async fn streams_in_batches_and_truncates_at_max_rows() {
    if !enabled() {
        return;
    }
    let conn = connect(&config()).await;
    let docs: Vec<String> = (0..1_500).map(|i| format!("{{ n: {i} }}")).collect();
    ok(&conn, &format!("db.nums.insertMany([{}])", docs.join(","))).await;
    let limits = QueryLimits {
        max_rows: 1_000,
        batch_rows: 100,
        timeout: None,
    };
    let events = run(&conn, "db.nums.find().sort({ n: 1 })", limits).await;
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
    let again = ok(&conn, "db.nums.countDocuments()").await;
    assert_eq!(rows(&again), vec![vec![CellValue::Signed(1_500)]]);
    drop_db(&conn).await;
}

#[tokio::test]
async fn cancel_and_timeout_are_prompt() {
    if !enabled() {
        return;
    }
    let conn = connect(&config()).await;
    ok(&conn, "db.slow.insertOne({ a: 1 })").await;
    // A `$where` that sleeps server-side keeps the operation busy.
    let slow = "db.slow.find({ $where: 'sleep(30000) || true' })";
    let mut exec = conn
        .execute(req(slow, QueryLimits::default()))
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

    // max_time stops the server-side work; reported as a timeout.
    let limits = QueryLimits {
        timeout: Some(Duration::from_millis(500)),
        ..QueryLimits::default()
    };
    let started = std::time::Instant::now();
    let events = run(&conn, slow, limits).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::TimedOut { .. })),
        "{events:?}"
    );
    assert!(started.elapsed() < Duration::from_secs(5));
    drop_db(&conn).await;
}

#[tokio::test]
async fn errors_are_structured_and_invalid_input_is_explained() {
    if !enabled() {
        return;
    }
    let conn = connect(&config()).await;
    ok(&conn, "db.u.insertOne({ _id: 1 })").await;
    let dup = run(&conn, "db.u.insertOne({ _id: 1 })", QueryLimits::default()).await;
    match dup.last() {
        Some(QueryEvent::Failed(e)) => {
            assert_eq!(e.kind, DbErrorKind::Query);
            assert_eq!(e.code.as_deref(), Some("11000"));
        }
        other => panic!("expected duplicate key, got {other:?}"),
    }
    let bad = run(&conn, "SELECT * FROM u", QueryLimits::default()).await;
    match bad.last() {
        Some(QueryEvent::Failed(e)) => {
            assert!(e.message.starts_with("Invalid query"), "{e:?}");
            assert!(e.hint.is_some());
        }
        other => panic!("expected parse failure, got {other:?}"),
    }
    let unknown = run(&conn, r#"{ "noSuchCommand": 1 }"#, QueryLimits::default()).await;
    assert!(matches!(unknown.last(), Some(QueryEvent::Failed(e)) if e.kind == DbErrorKind::Query));
    drop_db(&conn).await;
}

#[tokio::test]
async fn read_only_profile_refuses_writes_client_side() {
    if !enabled() {
        return;
    }
    let mut cfg = config();
    cfg.profile.read_only = true;
    let conn = connect(&cfg).await;
    let events = run(&conn, "db.u.insertOne({ a: 1 })", QueryLimits::default()).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::Failed(e)) if e.message.contains("read-only")),
        "{events:?}"
    );
    ok(&conn, "db.u.find()").await;
    conn.close().await;
}

#[tokio::test]
async fn lists_collections_and_views_and_runs_the_browse_query() {
    if !enabled() {
        return;
    }
    let conn = connect(&config()).await;
    ok(&conn, "db.User.insertOne({ orgId: 1 })").await;
    ok(&conn, "db.no_pk.insertOne({ x: 1 })").await;
    ok(
        &conn,
        r#"{ "create": "v_users", "viewOn": "User", "pipeline": [{ "$match": {} }] }"#,
    )
    .await;
    let tables = conn.list_tables().await.expect("list");
    let names: Vec<_> = tables.iter().map(|t| t.table.name.as_str()).collect();
    assert_eq!(names, vec!["User", "no_pk", "v_users"]);
    assert_eq!(tables[0].primary_key, vec!["_id".to_owned()]);
    assert_eq!(tables[2].kind, TableKind::View);
    let browse = ok(
        &conn,
        r#"db.getCollection("User").find({}).sort({ "_id": 1 }).limit(11)"#,
    )
    .await;
    assert_eq!(rows(&browse).len(), 1);
    drop_db(&conn).await;
}

//! Driver contract tests against a real PostgreSQL server.
//!
//! Skipped unless `FERRUMDATA_TEST_PG=1`. Start the server with
//! `docker compose up -d postgres`. Override the target with
//! `FERRUMDATA_TEST_PG_HOST`, `_PORT`, `_USER`, `_PASSWORD`, `_DB`.

use ferrumdata_db_driver_api::{DatabaseDriver, DbConnection};
use ferrumdata_db_postgres::PostgresDriver;
use ferrumdata_domain::{
    CellValue, ConnectionProfile, DbErrorKind, Endpoint, ExecutionId, ProfileId, QueryEvent,
    QueryLimits, QueryRequest, ResolvedConnectionConfig, Secret, TlsMode,
};
use std::sync::Arc;
use std::time::Duration;

fn env(key: &str, default: &str) -> String {
    std::env::var(format!("FERRUMDATA_TEST_PG_{key}")).unwrap_or_else(|_| default.to_owned())
}

fn enabled() -> bool {
    let on = std::env::var("FERRUMDATA_TEST_PG").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipping: set FERRUMDATA_TEST_PG=1 to run PostgreSQL integration tests");
    }
    on
}

fn config(password: &str, read_only: bool) -> ResolvedConnectionConfig {
    let mut profile = ConnectionProfile::new_postgres("it");
    profile.endpoint = Endpoint::Tcp {
        host: env("HOST", "127.0.0.1"),
        port: env("PORT", "54329").parse().expect("port"),
    };
    profile.username = Some(env("USER", "ferrumdata"));
    profile.database = Some(env("DB", "ferrumdata_test"));
    profile.tls = TlsMode::Disable;
    profile.read_only = read_only;
    ResolvedConnectionConfig {
        profile,
        password: Some(Secret::new(password)),
    }
}

async fn connect() -> Arc<dyn DbConnection> {
    PostgresDriver
        .connect(&config(&env("PASSWORD", "ferrumdata-dev-password"), false))
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

/// Runs to completion, returning all events.
async fn run(conn: &Arc<dyn DbConnection>, sql: &str, limits: QueryLimits) -> Vec<QueryEvent> {
    let mut exec = conn.execute(req(sql, limits)).await.expect("execute");
    let mut out = Vec::new();
    while let Some(e) = exec.events.recv().await {
        out.push(e);
    }
    out
}

fn rows(events: &[QueryEvent]) -> Vec<Vec<CellValue>> {
    events
        .iter()
        .filter_map(|e| {
            if let QueryEvent::Rows(r) = e {
                Some(r.clone())
            } else {
                None
            }
        })
        .flatten()
        .collect()
}

#[tokio::test]
async fn connects_and_reports_version() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    assert!(
        conn.server_info()
            .await
            .expect("info")
            .version
            .starts_with("PostgreSQL ")
    );
    conn.close().await;
}

#[tokio::test]
async fn bad_password_is_authentication_error_without_secret() {
    if !enabled() {
        return;
    }
    let err = PostgresDriver
        .connect(&config("wrong-pass-XYZ", false))
        .await
        .err()
        .expect("must fail");
    assert_eq!(err.kind, DbErrorKind::Authentication, "{err:?}");
    assert!(!format!("{err:?}").contains("wrong-pass-XYZ"));
}

#[tokio::test]
async fn decodes_types_and_distinguishes_null_from_empty() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    let sql = "select null::text, ''::text, 'héllo 😀'::text, 42::int4, 1.5::float8, \
               123456789012345678901234567890.123::numeric, 'NaN'::numeric, 1.50::numeric(10,2), true, '\\xdead'::bytea, \
               '2024-01-02 03:04:05+00'::timestamptz, '{\"a\":1}'::jsonb, \
               '00000000-0000-0000-0000-000000000001'::uuid, '2024-01-02'::date";
    let events = run(&conn, sql, QueryLimits::default()).await;
    let row = &rows(&events)[0];
    assert_eq!(row[0], CellValue::Null);
    assert_eq!(row[1], CellValue::Text(String::new()));
    assert_eq!(row[2], CellValue::Text("héllo 😀".into()));
    assert_eq!(row[3], CellValue::Signed(42));
    assert_eq!(row[4], CellValue::Float(1.5));
    assert_eq!(
        row[5],
        CellValue::Decimal("123456789012345678901234567890.123".into())
    );
    assert_eq!(row[6], CellValue::Decimal("NaN".into()));
    assert_eq!(row[7], CellValue::Decimal("1.50".into()));
    assert_eq!(row[8], CellValue::Bool(true));
    assert_eq!(row[9], CellValue::Binary(vec![0xde, 0xad]));
    assert_eq!(row[10].display(), "2024-01-02T03:04:05+00:00");
    assert_eq!(row[11], CellValue::Json("{\"a\":1}".into()));
    assert_eq!(
        row[12],
        CellValue::Uuid("00000000-0000-0000-0000-000000000001".into())
    );
    assert_eq!(row[13].display(), "2024-01-02");
    conn.close().await;
}

#[tokio::test]
async fn streams_in_batches_and_truncates_at_max_rows() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    let limits = QueryLimits {
        max_rows: 1_000,
        batch_rows: 100,
        timeout: None,
    };
    let events = run(&conn, "select g from generate_series(1, 100000) g", limits).await;
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
    // Pool still usable after the truncated (tainted) session was discarded.
    let again = run(&conn, "select 1", QueryLimits::default()).await;
    assert_eq!(rows(&again), vec![vec![CellValue::Signed(1)]]);
    conn.close().await;
}

#[tokio::test]
async fn cancel_stops_server_query_and_pool_recovers() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    let mut exec = conn
        .execute(req("select pg_sleep(30)", QueryLimits::default()))
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

    // The server-side backend must not still be sleeping.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let check = run(&conn, "select count(*)::int8 from pg_stat_activity where query = 'select pg_sleep(30)' and state = 'active'", QueryLimits::default()).await;
    assert_eq!(rows(&check), vec![vec![CellValue::Signed(0)]]);
    conn.close().await;
}

#[tokio::test]
async fn timeout_is_reported() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    let limits = QueryLimits {
        timeout: Some(Duration::from_millis(300)),
        ..QueryLimits::default()
    };
    let events = run(&conn, "select pg_sleep(10)", limits).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::TimedOut { .. })),
        "{events:?}"
    );
    conn.close().await;
}

#[tokio::test]
async fn ddl_and_dml_report_affected_rows_and_errors_are_structured() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    let table = format!("t_{}", ExecutionId::new().0.simple());
    run(
        &conn,
        &format!("create table {table} (id int primary key)"),
        QueryLimits::default(),
    )
    .await;
    let ins = run(
        &conn,
        &format!("insert into {table} select generate_series(1, 3)"),
        QueryLimits::default(),
    )
    .await;
    assert!(matches!(ins.last(), Some(QueryEvent::Completed(o)) if o.rows_affected == Some(3)));
    let dup = run(
        &conn,
        &format!("insert into {table} values (1)"),
        QueryLimits::default(),
    )
    .await;
    match dup.last() {
        Some(QueryEvent::Failed(e)) => {
            assert_eq!(e.kind, DbErrorKind::Query);
            assert_eq!(e.code.as_deref(), Some("23505"));
        }
        other => panic!("expected unique violation, got {other:?}"),
    }
    run(
        &conn,
        &format!("drop table {table}"),
        QueryLimits::default(),
    )
    .await;
    conn.close().await;
}

#[tokio::test]
async fn read_only_profile_rejects_writes() {
    if !enabled() {
        return;
    }
    let conn = PostgresDriver
        .connect(&config(&env("PASSWORD", "ferrumdata-dev-password"), true))
        .await
        .expect("connect");
    let events = run(
        &conn,
        "create table should_not_exist (id int)",
        QueryLimits::default(),
    )
    .await;
    match events.last() {
        Some(QueryEvent::Failed(e)) => assert_eq!(e.code.as_deref(), Some("25006")), // read_only_sql_transaction
        other => panic!("expected read-only failure, got {other:?}"),
    }
    conn.close().await;
}

#[tokio::test]
async fn lists_tables_with_case_preserved_names_and_primary_keys() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    let schema = format!("s_{}", ExecutionId::new().0.simple());
    let setup = [
        format!("create schema {schema}"),
        format!(
            "create table {schema}.\"User\" (\"orgId\" int, id text, primary key (\"orgId\", id))"
        ),
        format!("create table {schema}.no_pk (x int)"),
        format!("create view {schema}.v_users as select * from {schema}.\"User\""),
    ];
    for sql in &setup {
        run(&conn, sql, QueryLimits::default()).await;
    }
    let tables: Vec<_> = conn
        .list_tables()
        .await
        .expect("list")
        .into_iter()
        .filter(|t| t.table.schema.as_deref() == Some(schema.as_str()))
        .collect();
    let names: Vec<_> = tables.iter().map(|t| t.table.name.as_str()).collect();
    assert_eq!(names, vec!["User", "no_pk", "v_users"]);
    assert_eq!(
        tables[0].primary_key,
        vec!["orgId".to_owned(), "id".to_owned()]
    );
    assert!(tables[1].primary_key.is_empty());
    assert_eq!(tables[2].kind, ferrumdata_domain::TableKind::View);
    assert!(
        !conn
            .list_tables()
            .await
            .expect("list")
            .iter()
            .any(|t| t.table.schema.as_deref() == Some("pg_catalog"))
    );
    run(
        &conn,
        &format!("drop schema {schema} cascade"),
        QueryLimits::default(),
    )
    .await;
    conn.close().await;
}

#[tokio::test]
async fn describes_columns_and_indexes() {
    if !enabled() {
        return;
    }
    let conn = connect().await;
    let schema = format!("s_{}", ExecutionId::new().0.simple());
    let setup = [
        format!("create schema {schema}"),
        format!(
            "create table {schema}.\"Order\" (
                id bigserial,
                \"customerEmail\" varchar(80) not null default 'n/a',
                total numeric(10, 2),
                created timestamptz default now(),
                region text,
                primary key (region, id)
            )"
        ),
        format!(
            "create unique index order_email on {schema}.\"Order\" (lower(\"customerEmail\"), total) include (created)"
        ),
        format!("create index order_created on {schema}.\"Order\" (created)"),
    ];
    for sql in &setup {
        let events = run(&conn, sql, QueryLimits::default()).await;
        assert!(
            matches!(events.last(), Some(QueryEvent::Completed(_))),
            "{sql}: {events:?}"
        );
    }
    let table = ferrumdata_domain::TableRef {
        schema: Some(schema.clone()),
        name: "Order".into(),
    };
    let d = conn.describe_table(&table).await.expect("describe");
    let cols: Vec<String> = d
        .columns
        .iter()
        .map(|c| format!("{} {}", c.name, c.summary()))
        .collect();
    assert_eq!(
        cols,
        vec![
            format!("id bigint · PK · NOT NULL · = nextval('{schema}.\"Order_id_seq\"'::regclass)"),
            "customerEmail character varying(80) · NOT NULL · = 'n/a'::character varying"
                .to_owned(),
            "total numeric(10,2) · NULL".to_owned(),
            "created timestamp with time zone · NULL · = now()".to_owned(),
            "region text · PK · NOT NULL".to_owned(),
        ]
    );
    let idx: Vec<String> = d
        .indexes
        .iter()
        .map(|i| format!("{} {}", i.name, i.summary()))
        .collect();
    assert_eq!(
        idx,
        vec![
            "Order_pkey PRIMARY KEY (region, id)",
            "order_created (created)",
            "order_email UNIQUE (lower(\"customerEmail\"::text), total)",
        ]
    );
    run(
        &conn,
        &format!("drop schema {schema} cascade"),
        QueryLimits::default(),
    )
    .await;
    conn.close().await;
}

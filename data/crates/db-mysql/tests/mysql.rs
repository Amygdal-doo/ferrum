//! Driver contract tests against a real MySQL server.
//!
//! Skipped unless `FERRUMDATA_TEST_MYSQL=1`. Start the server with
//! `docker compose up -d mysql`. Override the target with
//! `FERRUMDATA_TEST_MYSQL_HOST`, `_PORT`, `_USER`, `_PASSWORD`, `_DB`.

use ferrumdata_db_driver_api::{DatabaseDriver, DbConnection, ScriptStatement};
use ferrumdata_db_mysql::MySqlDriver;
use ferrumdata_domain::{
    CellValue, ConnectionProfile, DatabaseEngine, DbErrorKind, Endpoint, ExecutionId, ProfileId,
    QueryEvent, QueryLimits, QueryRequest, ResolvedConnectionConfig, Secret, TableKind, TableRef,
    TlsMode,
};
use std::sync::Arc;
use std::time::Duration;

fn env(key: &str, default: &str) -> String {
    std::env::var(format!("FERRUMDATA_TEST_MYSQL_{key}")).unwrap_or_else(|_| default.to_owned())
}

fn enabled() -> bool {
    let on = std::env::var("FERRUMDATA_TEST_MYSQL").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipping: set FERRUMDATA_TEST_MYSQL=1 to run MySQL integration tests");
    }
    on
}

fn config(password: &str, read_only: bool) -> ResolvedConnectionConfig {
    let mut profile = ConnectionProfile::new(DatabaseEngine::MySql, "it");
    profile.endpoint = Endpoint::Tcp {
        host: env("HOST", "127.0.0.1"),
        port: env("PORT", "33069").parse().expect("port"),
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

async fn connect_with(read_only: bool) -> Arc<dyn DbConnection> {
    MySqlDriver
        .connect(&config(
            &env("PASSWORD", "ferrumdata-dev-password"),
            read_only,
        ))
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

fn unique(prefix: &str) -> String {
    format!("{prefix}_{}", ExecutionId::new().0.simple())
}

#[tokio::test]
async fn connects_and_reports_version() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let info = conn.server_info().await.expect("info");
    assert!(
        info.version.starts_with("MySQL ") || info.version.starts_with("MariaDB "),
        "{info:?}"
    );
    conn.close().await;
}

#[tokio::test]
async fn bad_password_is_authentication_error_without_secret() {
    if !enabled() {
        return;
    }
    let err = MySqlDriver
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
    let conn = connect_with(false).await;
    let t = unique("types");
    ok(
        &conn,
        &format!(
            "create table {t} (n varchar(10), e varchar(10), u varchar(20), i int, \
             big bigint unsigned, f double, d decimal(40,3), b tinyint(1), bin varbinary(4), \
             ts timestamp, dt datetime, j json, day date, tm time, en enum('a','b'))"
        ),
    )
    .await;
    ok(
        &conn,
        &format!(
            "insert into {t} values (null, '', 'héllo 😀', -42, 18446744073709551615, 1.5, \
             123456789012345678901234567890.123, 1, x'dead', '2024-01-02 03:04:05', \
             '2024-01-02 03:04:05', '{{\"a\":1}}', '2024-01-02', '-01:02:03', 'b')"
        ),
    )
    .await;
    let events = run(&conn, &format!("select * from {t}"), QueryLimits::default()).await;
    let row = &rows(&events)[0];
    assert_eq!(row[0], CellValue::Null);
    assert_eq!(row[1], CellValue::Text(String::new()));
    assert_eq!(row[2], CellValue::Text("héllo 😀".into()));
    assert_eq!(row[3], CellValue::Signed(-42));
    assert_eq!(row[4], CellValue::Unsigned(u64::MAX));
    assert_eq!(row[5], CellValue::Float(1.5));
    assert_eq!(
        row[6],
        CellValue::Decimal("123456789012345678901234567890.123".into())
    );
    assert_eq!(row[7], CellValue::Bool(true));
    assert_eq!(row[8], CellValue::Binary(vec![0xde, 0xad]));
    assert_eq!(row[9].display(), "2024-01-02T03:04:05+00:00");
    assert_eq!(row[10].display(), "2024-01-02 03:04:05");
    assert_eq!(row[11], CellValue::Json("{\"a\":1}".into()));
    assert_eq!(row[12].display(), "2024-01-02");
    // A negative TIME is an interval, never silently shown as 01:02:03.
    assert_eq!(row[13], CellValue::Text("-01:02:03".into()));
    assert_eq!(row[14], CellValue::Text("b".into()));
    ok(&conn, &format!("drop table {t}")).await;
    conn.close().await;
}

#[tokio::test]
async fn streams_in_batches_and_truncates_at_max_rows() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let limits = QueryLimits {
        max_rows: 1_000,
        batch_rows: 100,
        timeout: None,
    };
    // ~100k rows from a cross join of a catalog table with itself.
    let sql = "select a.ORDINAL_POSITION from information_schema.COLUMNS a \
               cross join information_schema.COLUMNS b limit 100000";
    let events = run(&conn, sql, limits).await;
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
async fn cancel_stops_server_query_and_pool_recovers() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let mut exec = conn
        .execute(req("select sleep(30)", QueryLimits::default()))
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
    tokio::time::sleep(Duration::from_millis(300)).await;
    let check = run(
        &conn,
        "select count(*) from information_schema.PROCESSLIST where INFO = 'select sleep(30)'",
        QueryLimits::default(),
    )
    .await;
    assert_eq!(rows(&check), vec![vec![CellValue::Signed(0)]]);
    conn.close().await;
}

#[tokio::test]
async fn timeout_is_reported() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let limits = QueryLimits {
        timeout: Some(Duration::from_millis(300)),
        ..QueryLimits::default()
    };
    let events = run(&conn, "select sleep(10)", limits).await;
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
    let conn = connect_with(false).await;
    let t = unique("t");
    ok(&conn, &format!("create table {t} (id int primary key)")).await;
    let ins = run(
        &conn,
        &format!("insert into {t} values (1), (2), (3)"),
        QueryLimits::default(),
    )
    .await;
    assert!(matches!(ins.last(), Some(QueryEvent::Completed(o)) if o.rows_affected == Some(3)));
    let dup = run(
        &conn,
        &format!("insert into {t} values (1)"),
        QueryLimits::default(),
    )
    .await;
    match dup.last() {
        Some(QueryEvent::Failed(e)) => {
            assert_eq!(e.kind, DbErrorKind::Query);
            assert_eq!(e.code.as_deref(), Some("1062")); // ER_DUP_ENTRY
            assert_eq!(e.detail.as_deref(), Some("SQLSTATE 23000"));
        }
        other => panic!("expected duplicate key, got {other:?}"),
    }
    // Not preparable: exercises the text-protocol fallback.
    let procedure = unique("p");
    ok(&conn, &format!("create procedure {procedure}() select 1")).await;
    ok(&conn, &format!("drop procedure {procedure}")).await;
    ok(&conn, &format!("drop table {t}")).await;
    conn.close().await;
}

#[tokio::test]
async fn read_only_profile_rejects_writes() {
    if !enabled() {
        return;
    }
    let rw = connect_with(false).await;
    let t = unique("ro");
    ok(&rw, &format!("create table {t} (id int)")).await;
    let ro = connect_with(true).await;
    let events = run(
        &ro,
        &format!("insert into {t} values (1)"),
        QueryLimits::default(),
    )
    .await;
    match events.last() {
        // ER_CANT_EXECUTE_IN_READ_ONLY_TRANSACTION
        Some(QueryEvent::Failed(e)) => assert_eq!(e.code.as_deref(), Some("1792"), "{e:?}"),
        other => panic!("expected read-only failure, got {other:?}"),
    }
    ro.close().await;
    ok(&rw, &format!("drop table {t}")).await;
    rw.close().await;
}

#[tokio::test]
async fn lists_tables_with_case_preserved_names_and_primary_keys() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let suffix = ExecutionId::new().0.simple().to_string();
    let user = format!("User_{suffix}");
    let no_pk = format!("no_pk_{suffix}");
    let view = format!("v_users_{suffix}");
    ok(
        &conn,
        &format!("create table `{user}` (`orgId` int, id varchar(10), primary key (`orgId`, id))"),
    )
    .await;
    ok(&conn, &format!("create table {no_pk} (x int)")).await;
    ok(
        &conn,
        &format!("create view {view} as select * from `{user}`"),
    )
    .await;
    let tables: Vec<_> = conn
        .list_tables()
        .await
        .expect("list")
        .into_iter()
        .filter(|t| t.table.name.ends_with(&suffix))
        .collect();
    let names: Vec<_> = tables.iter().map(|t| t.table.name.as_str()).collect();
    assert_eq!(names, vec![user.as_str(), no_pk.as_str(), view.as_str()]);
    assert_eq!(tables[0].table.schema.as_deref(), Some("ferrumdata_test"));
    assert_eq!(
        tables[0].primary_key,
        vec!["orgId".to_owned(), "id".to_owned()]
    );
    assert!(tables[1].primary_key.is_empty());
    assert_eq!(tables[2].kind, TableKind::View);
    let all = conn.list_tables().await.expect("list");
    assert!(
        !all.iter()
            .any(|t| t.table.schema.as_deref() == Some("mysql"))
    );
    ok(&conn, &format!("drop view {view}")).await;
    ok(&conn, &format!("drop table {no_pk}")).await;
    ok(&conn, &format!("drop table `{user}`")).await;
    conn.close().await;
}

#[tokio::test]
async fn describes_columns_and_indexes() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let name = unique("Order");
    ok(
        &conn,
        &format!(
            "create table `{name}` (
                id int unsigned not null auto_increment,
                `customerEmail` varchar(80) not null default 'n/a',
                total decimal(10, 2),
                created timestamp null default current_timestamp,
                primary key (id),
                unique key order_email (`customerEmail`, total),
                key order_lower ((lower(`customerEmail`)))
            )"
        ),
    )
    .await;
    let table = TableRef {
        schema: None,
        name: name.clone(),
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
            "id int unsigned auto_increment · PK · NOT NULL",
            "customerEmail varchar(80) · NOT NULL · = n/a",
            "total decimal(10,2) · NULL",
            "created timestamp · NULL · = CURRENT_TIMESTAMP",
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
            "PRIMARY PRIMARY KEY (id)",
            "order_email UNIQUE (customerEmail, total)",
            "order_lower (<expression>)",
        ]
    );
    // Explicit schema works too.
    let qualified = TableRef {
        schema: Some("ferrumdata_test".into()),
        name: name.clone(),
    };
    assert_eq!(conn.describe_table(&qualified).await.expect("describe"), d);
    ok(&conn, &format!("drop table `{name}`")).await;
    conn.close().await;
}

#[tokio::test]
async fn scripts_are_atomic_and_literals_round_trip() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let table = format!("s_{}", ExecutionId::new().0.simple());
    let engine = DatabaseEngine::MySql;
    let tricky = "it's a \\ back\\slash";
    let create = format!("create table {table} (id int primary key, v text)");
    conn.execute_script(&[ScriptStatement::new(create)])
        .await
        .expect("create");
    let insert = format!(
        "insert into {table} (id, v) values (1, {})",
        engine.string_literal(tricky)
    );
    let counts = conn
        .execute_script(&[ScriptStatement {
            sql: insert,
            expect_rows: Some(1),
        }])
        .await
        .expect("insert");
    assert_eq!(counts, vec![1]);
    let err = conn
        .execute_script(&[
            ScriptStatement::new(format!("delete from {table} where id = 1")),
            ScriptStatement {
                sql: format!("delete from {table} where id = 2"),
                expect_rows: Some(1),
            },
        ])
        .await
        .expect_err("expected count");
    assert_eq!(err.kind, DbErrorKind::Query);
    let events = run(
        &conn,
        &format!("select v from {table}"),
        QueryLimits::default(),
    )
    .await;
    assert_eq!(rows(&events), vec![vec![CellValue::Text(tricky.into())]]);
    conn.execute_script(&[ScriptStatement::new(format!("drop table {table}"))])
        .await
        .expect("drop");
    conn.close().await;
}

#[tokio::test]
async fn describes_composite_foreign_keys() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let parent = unique("fkp");
    let child = unique("fkc");
    ok(
        &conn,
        &format!("create table {parent} (a int, b int, primary key (a, b))"),
    )
    .await;
    ok(
        &conn,
        &format!(
            "create table {child} (id int primary key, x int, y int, \
             constraint {child}_fk foreign key (x, y) references {parent} (a, b) on delete cascade)"
        ),
    )
    .await;
    let details = conn
        .describe_table(&TableRef {
            schema: None,
            name: child.clone(),
        })
        .await
        .expect("describe");
    let fk = &details.foreign_keys[0];
    assert_eq!(fk.name, format!("{child}_fk"));
    assert_eq!(fk.columns, vec!["x", "y"]);
    assert_eq!(fk.ref_table.name, parent);
    assert_eq!(fk.ref_columns, vec!["a", "b"]);
    assert_eq!(fk.on_delete, "CASCADE");
    ok(&conn, &format!("drop table {child}")).await;
    ok(&conn, &format!("drop table {parent}")).await;
    conn.close().await;
}

/// EXPLAIN reports no columns when prepared; its rows must still arrive.
#[tokio::test]
async fn explain_streams_rows_known_only_after_execution() {
    if !enabled() {
        return;
    }
    let conn = connect_with(false).await;
    let events = run(
        &conn,
        "EXPLAIN FORMAT=TREE SELECT 1",
        QueryLimits::default(),
    )
    .await;
    assert!(
        events
            .iter()
            .any(|e| matches!(e, QueryEvent::Metadata(c) if c.len() == 1)),
        "{events:?}"
    );
    assert_eq!(rows(&events).len(), 1, "{events:?}");
    conn.close().await;
}

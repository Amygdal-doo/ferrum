//! Driver contract tests against a real Redis server.
//!
//! Skipped unless `FERRUMDATA_TEST_REDIS=1`. Start the server with
//! `docker compose up -d redis`. Override the target with
//! `FERRUMDATA_TEST_REDIS_HOST`, `_PORT`, `_PASSWORD`, `_DB`.
//! Each test writes only keys under its own random prefix and removes them.

use ferrumdata_db_driver_api::{DatabaseDriver, DbConnection, KeyValueStore};
use ferrumdata_db_redis::RedisDriver;
use ferrumdata_domain::{
    CellValue, ConnectionProfile, DatabaseEngine, DbErrorKind, Endpoint, ExecutionId, KeyKind,
    KeyTtl, ProfileId, QueryEvent, QueryLimits, QueryRequest, ResolvedConnectionConfig, Secret,
};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

fn env(key: &str, default: &str) -> String {
    std::env::var(format!("FERRUMDATA_TEST_REDIS_{key}")).unwrap_or_else(|_| default.to_owned())
}

fn enabled() -> bool {
    let on = std::env::var("FERRUMDATA_TEST_REDIS").is_ok_and(|v| v == "1");
    if !on {
        eprintln!("skipping: set FERRUMDATA_TEST_REDIS=1 to run Redis integration tests");
    }
    on
}

fn config(password: &str, read_only: bool) -> ResolvedConnectionConfig {
    let mut profile = ConnectionProfile::new(DatabaseEngine::Redis, "it");
    profile.endpoint = Endpoint::Tcp {
        host: env("HOST", "127.0.0.1"),
        port: env("PORT", "63799").parse().expect("port"),
    };
    profile.database = Some(env("DB", "0"));
    profile.read_only = read_only;
    profile.connect_timeout_secs = 3;
    ResolvedConnectionConfig {
        profile,
        password: Some(Secret::new(password)),
    }
}

fn good(read_only: bool) -> ResolvedConnectionConfig {
    config(&env("PASSWORD", "ferrumdata-dev-password"), read_only)
}

async fn connect(read_only: bool) -> Arc<dyn DbConnection> {
    RedisDriver
        .connect(&good(read_only))
        .await
        .expect("connect")
}

fn kv(conn: &Arc<dyn DbConnection>) -> &dyn KeyValueStore {
    conn.key_value().expect("key-value store")
}

fn prefix() -> String {
    format!("ferrumdata-it:{}", ExecutionId::new().0.simple())
}

fn req(cmd: &str, limits: QueryLimits) -> QueryRequest {
    QueryRequest {
        execution_id: ExecutionId::new(),
        profile_id: ProfileId::new(),
        editor_id: None,
        sql: cmd.into(),
        limits,
    }
}

async fn run(conn: &Arc<dyn DbConnection>, cmd: &str, limits: QueryLimits) -> Vec<QueryEvent> {
    let mut exec = conn.execute(req(cmd, limits)).await.expect("execute");
    let mut out = Vec::new();
    while let Some(e) = exec.events.recv().await {
        out.push(e);
    }
    out
}

async fn ok(conn: &Arc<dyn DbConnection>, cmd: &str) -> Vec<QueryEvent> {
    let events = run(conn, cmd, QueryLimits::default()).await;
    assert!(
        matches!(events.last(), Some(QueryEvent::Completed(_))),
        "{cmd}: {events:?}"
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

async fn cleanup(conn: &Arc<dyn DbConnection>, prefix: &str) {
    let store = kv(conn);
    let mut cursor = 0;
    loop {
        let page = store
            .scan_keys(&format!("{prefix}*"), cursor, 500)
            .await
            .expect("scan");
        for k in &page.keys {
            store.delete_key(&k.name).await.expect("delete");
        }
        cursor = page.cursor;
        if cursor == 0 {
            break;
        }
    }
    conn.close().await;
}

/// One key of every type under `p`.
async fn seed(conn: &Arc<dyn DbConnection>, p: &str) {
    for cmd in [
        format!(r#"SET {p}:str "{{\"name\":\"Ann\",\"tags\":[1,2]}}" EX 600"#),
        format!("HSET {p}:hash f1 v1 f2 v2 f3 v3"),
        format!("RPUSH {p}:list a b c d"),
        format!("SADD {p}:set x y"),
        format!("ZADD {p}:zset 1.5 low 10 high"),
        format!("XADD {p}:stream 1-0 temp 20 unit C"),
        format!("XADD {p}:stream 2-0 temp 21"),
    ] {
        ok(conn, &cmd).await;
    }
}

#[tokio::test]
async fn connects_and_reports_version() {
    if !enabled() {
        return;
    }
    let conn = connect(false).await;
    let info = conn.server_info().await.expect("info");
    assert!(
        info.version.starts_with("Redis ") || info.version.starts_with("Valkey "),
        "{info:?}"
    );
    assert!(conn.list_tables().await.expect("tables").is_empty());
    conn.close().await;
}

#[tokio::test]
async fn bad_password_and_unreachable_host_are_classified_without_secret() {
    if !enabled() {
        return;
    }
    let err = RedisDriver
        .connect(&config("wrong-pass-XYZ", false))
        .await
        .err()
        .expect("must fail");
    assert_eq!(err.kind, DbErrorKind::Authentication, "{err:?}");
    assert!(!format!("{err:?}").contains("wrong-pass-XYZ"));

    let mut cfg = good(false);
    cfg.profile.endpoint = Endpoint::Tcp {
        host: "127.0.0.1".into(),
        port: 1,
    };
    let err = RedisDriver.connect(&cfg).await.err().expect("must fail");
    assert_eq!(err.kind, DbErrorKind::Connection, "{err:?}");
}

#[tokio::test]
async fn scan_pages_list_every_key_with_type_and_ttl() {
    if !enabled() {
        return;
    }
    let conn = connect(false).await;
    let p = prefix();
    seed(&conn, &p).await;
    let store = kv(&conn);
    assert!(store.key_count().await.expect("dbsize") >= 6);

    let mut found = HashMap::new();
    let mut cursor = 0;
    let mut pages = 0;
    loop {
        let page = store
            .scan_keys(&format!("{p}:*"), cursor, 2)
            .await
            .expect("scan");
        pages += 1;
        for k in page.keys {
            found.insert(k.name.clone(), k);
        }
        cursor = page.cursor;
        if cursor == 0 {
            break;
        }
        assert!(pages < 1000, "scan did not terminate");
    }
    assert_eq!(found.len(), 6, "{found:?}");
    let kind = |s: &str| found[&format!("{p}:{s}")].kind.clone();
    assert_eq!(kind("str"), KeyKind::String);
    assert_eq!(kind("hash"), KeyKind::Hash);
    assert_eq!(kind("list"), KeyKind::List);
    assert_eq!(kind("set"), KeyKind::Set);
    assert_eq!(kind("zset"), KeyKind::SortedSet);
    assert_eq!(kind("stream"), KeyKind::Stream);
    assert!(matches!(
        found[&format!("{p}:str")].ttl,
        KeyTtl::ExpiresIn(d) if d > Duration::from_secs(500)
    ));
    assert_eq!(found[&format!("{p}:hash")].ttl, KeyTtl::Persistent);
    // A pattern that matches nothing completes with no keys.
    let none = store
        .scan_keys(&format!("{p}:nope*"), 0, 50)
        .await
        .expect("scan");
    assert!(none.keys.is_empty());
    cleanup(&conn, &p).await;
}

#[tokio::test]
async fn inspects_each_type_as_grid_rows() {
    if !enabled() {
        return;
    }
    let conn = connect(false).await;
    let p = prefix();
    seed(&conn, &p).await;
    let store = kv(&conn);

    let s = store
        .inspect_key(&format!("{p}:str"), 100)
        .await
        .expect("str");
    assert_eq!(s.kind, KeyKind::String);
    assert!(s.is_json);
    assert!(
        s.text
            .as_deref()
            .is_some_and(|t| t.contains("\n  \"name\": \"Ann\""))
    );
    assert!(s.memory_bytes.is_some_and(|m| m > 0), "{s:?}");
    assert!(matches!(s.ttl, KeyTtl::ExpiresIn(_)));

    let h = store
        .inspect_key(&format!("{p}:hash"), 100)
        .await
        .expect("hash");
    assert_eq!(h.length, Some(3));
    assert_eq!(h.columns[0].name, "field");
    let mut fields: Vec<_> = h.rows.iter().map(|r| r[0].clone()).collect();
    fields.sort_by_key(CellValue::display);
    assert_eq!(fields[0], CellValue::Text("f1".into()));
    assert!(!h.truncated);

    let l = store
        .inspect_key(&format!("{p}:list"), 2)
        .await
        .expect("list");
    assert_eq!(l.rows.len(), 2);
    assert_eq!(
        l.rows[1],
        vec![CellValue::Signed(1), CellValue::Text("b".into())]
    );
    assert!(l.truncated);
    assert_eq!(l.length, Some(4));

    let set = store
        .inspect_key(&format!("{p}:set"), 100)
        .await
        .expect("set");
    assert_eq!(set.rows.len(), 2);

    let z = store
        .inspect_key(&format!("{p}:zset"), 100)
        .await
        .expect("zset");
    assert_eq!(
        z.rows[0],
        vec![CellValue::Text("low".into()), CellValue::Float(1.5)]
    );

    let x = store
        .inspect_key(&format!("{p}:stream"), 100)
        .await
        .expect("stream");
    let names: Vec<_> = x.columns.iter().map(|c| c.name.as_str()).collect();
    assert_eq!(names, ["id", "temp", "unit"]);
    assert_eq!(x.rows.len(), 2);

    let gone = store
        .inspect_key(&format!("{p}:missing"), 10)
        .await
        .expect("missing");
    assert_eq!(gone.kind, KeyKind::Missing);
    assert_eq!(gone.ttl, KeyTtl::Missing);
    cleanup(&conn, &p).await;
}

#[tokio::test]
async fn edits_keep_ttl_and_are_refused_on_read_only_profiles() {
    if !enabled() {
        return;
    }
    let conn = connect(false).await;
    let p = prefix();
    seed(&conn, &p).await;
    let store = kv(&conn);
    let key = format!("{p}:str");

    store.set_string(&key, "plain text").await.expect("set");
    let v = store.inspect_key(&key, 10).await.expect("inspect");
    assert_eq!(v.text.as_deref(), Some("plain text"));
    assert!(!v.is_json);
    assert!(matches!(v.ttl, KeyTtl::ExpiresIn(_)), "KEEPTTL: {v:?}");

    assert!(store.set_ttl(&key, None).await.expect("persist"));
    assert_eq!(
        store.inspect_key(&key, 10).await.expect("inspect").ttl,
        KeyTtl::Persistent
    );
    assert!(
        store
            .set_ttl(&key, Some(Duration::from_secs(120)))
            .await
            .expect("expire")
    );
    assert!(matches!(
        store.inspect_key(&key, 10).await.expect("inspect").ttl,
        KeyTtl::ExpiresIn(d) if d <= Duration::from_secs(120)
    ));

    let ro = connect(true).await;
    let ro_store = kv(&ro);
    for err in [
        ro_store.set_string(&key, "x").await.err(),
        ro_store.set_ttl(&key, None).await.err(),
        ro_store.delete_key(&key).await.err(),
    ] {
        let err = err.expect("refused");
        assert!(err.message.contains("read-only"), "{err:?}");
    }
    // Reads still work on the read-only profile.
    assert_eq!(
        ro_store.inspect_key(&key, 10).await.expect("read").kind,
        KeyKind::String
    );
    ro.close().await;

    assert!(store.delete_key(&key).await.expect("delete"));
    assert!(!store.delete_key(&key).await.expect("delete again"));
    cleanup(&conn, &p).await;
}

#[tokio::test]
async fn console_maps_replies_to_the_grid() {
    if !enabled() {
        return;
    }
    let conn = connect(false).await;
    let p = prefix();
    seed(&conn, &p).await;

    let e = ok(&conn, &format!("HGETALL {p}:hash")).await;
    assert_eq!(column_names(&e), ["field", "value"]);
    assert_eq!(rows(&e).len(), 3);

    let e = ok(&conn, &format!("GET {p}:missing")).await;
    assert_eq!(rows(&e), vec![vec![CellValue::Null]]);

    let e = ok(&conn, &format!("ZRANGE {p}:zset 0 -1 WITHSCORES")).await;
    assert_eq!(column_names(&e), ["member", "score"]);
    assert_eq!(
        rows(&e)[1],
        vec![CellValue::Text("high".into()), CellValue::Float(10.0)]
    );

    let e = ok(&conn, &format!("LRANGE {p}:list 0 -1")).await;
    assert_eq!(column_names(&e), ["#", "value"]);
    assert_eq!(rows(&e).len(), 4);

    let e = ok(&conn, "INFO server").await;
    assert_eq!(column_names(&e), ["section", "field", "value"]);
    assert!(
        rows(&e)
            .iter()
            .any(|r| r[1] == CellValue::Text("redis_version".into()))
    );

    let e = ok(&conn, &format!("SET \"{p}:with space\" \"a b\"")).await;
    assert_eq!(rows(&e), vec![vec![CellValue::Text("OK".into())]]);
    let e = ok(&conn, &format!("GET \"{p}:with space\"")).await;
    assert_eq!(rows(&e), vec![vec![CellValue::Text("a b".into())]]);

    // Server errors fail the run with the Redis error code.
    let e = run(&conn, &format!("LPUSH {p}:hash x"), QueryLimits::default()).await;
    match e.last() {
        Some(QueryEvent::Failed(err)) => {
            assert_eq!(err.kind, DbErrorKind::Query);
            assert_eq!(err.code.as_deref(), Some("WRONGTYPE"));
        }
        other => panic!("{other:?}"),
    }

    // The row limit truncates long replies.
    let limits = QueryLimits {
        max_rows: 2,
        ..QueryLimits::default()
    };
    let e = run(&conn, &format!("LRANGE {p}:list 0 -1"), limits).await;
    assert_eq!(rows(&e).len(), 2);
    assert!(matches!(e.last(), Some(QueryEvent::Completed(o)) if o.truncated));
    cleanup(&conn, &p).await;
}

#[tokio::test]
async fn read_only_console_refuses_writes_with_a_clear_message() {
    if !enabled() {
        return;
    }
    let conn = connect(true).await;
    let p = prefix();
    let e = run(&conn, &format!("SET {p}:x 1"), QueryLimits::default()).await;
    match e.last() {
        Some(QueryEvent::Failed(err)) => {
            assert!(err.message.contains("read-only"), "{err:?}");
            assert!(err.message.contains("SET"), "{err:?}");
        }
        other => panic!("{other:?}"),
    }
    let e = run(&conn, "FLUSHDB", QueryLimits::default()).await;
    assert!(matches!(e.last(), Some(QueryEvent::Failed(_))), "{e:?}");
    ok(&conn, &format!("GET {p}:x")).await;
    ok(&conn, "CONFIG GET maxmemory").await;
    let e = run(&conn, "SUBSCRIBE news", QueryLimits::default()).await;
    assert!(
        matches!(e.last(), Some(QueryEvent::Failed(err)) if err.kind == DbErrorKind::Unsupported),
        "{e:?}"
    );
    conn.close().await;
}

#[tokio::test]
async fn blocking_commands_can_be_cancelled_and_time_out() {
    if !enabled() {
        return;
    }
    let conn = connect(false).await;
    let p = prefix();
    let mut exec = conn
        .execute(req(&format!("BLPOP {p}:never 0"), QueryLimits::default()))
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
    let e = run(&conn, &format!("BLPOP {p}:never 0"), limits).await;
    assert!(
        matches!(e.last(), Some(QueryEvent::TimedOut { .. })),
        "{e:?}"
    );

    // Dropping the run's connection unblocked the server side too.
    tokio::time::sleep(Duration::from_millis(300)).await;
    let e = ok(&conn, "INFO clients").await;
    let blocked = rows(&e)
        .into_iter()
        .find(|r| r[1] == CellValue::Text("blocked_clients".into()))
        .map(|r| r[2].clone());
    assert_eq!(blocked, Some(CellValue::Text("0".into())));

    // The key browser connection is unaffected.
    assert!(kv(&conn).key_count().await.is_ok());
    conn.close().await;
}

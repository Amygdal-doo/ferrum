use super::*;
use ferrumdata_domain::{EditorTabId, SecretRef, Theme};

#[tokio::test]
async fn settings_default_then_roundtrip() {
    let store = AppStore::in_memory().await.expect("open");
    assert_eq!(
        store.load_settings().await.expect("load"),
        AppSettings::default()
    );
    let mut s = AppSettings {
        theme: Theme::Dark,
        ..AppSettings::default()
    };
    s.layout.sidebar_width = 333.0;
    store.save_settings(&s).await.expect("save");
    store.save_settings(&s).await.expect("save twice upserts");
    assert_eq!(store.load_settings().await.expect("load"), s);
}

#[tokio::test]
async fn profiles_crud_and_no_secret_columns() {
    let store = AppStore::in_memory().await.expect("open");
    let mut p = ConnectionProfile::new_postgres("dev");
    p.secret_ref = Some(SecretRef::for_profile(p.id));
    store.upsert_profile(&p).await.expect("insert");
    p.display_name = "renamed".into();
    store.upsert_profile(&p).await.expect("update");
    let all = store.list_profiles().await.expect("list");
    assert_eq!(all, vec![p.clone()]);

    let raw: String = sqlx::query_scalar("SELECT data FROM connection_profiles")
        .fetch_one(&store.pool)
        .await
        .expect("raw");
    assert!(!raw.to_lowercase().contains("password"));

    store.delete_profile(p.id).await.expect("delete");
    assert!(store.list_profiles().await.expect("list").is_empty());
}

#[tokio::test]
async fn tabs_are_replaced_atomically_and_ordered() {
    let store = AppStore::in_memory().await.expect("open");
    let tab = |pos: i64| EditorTabState {
        id: EditorTabId::new(),
        title: format!("SQL {pos}"),
        sql: format!("select {pos};"),
        profile_id: None,
        position: pos,
        table: (pos == 7).then(|| ferrumdata_domain::TableRef {
            schema: Some("public".into()),
            name: "User".into(),
        }),
    };
    store.save_tabs(&[tab(2), tab(1)]).await.expect("save");
    let loaded = store.load_tabs().await.expect("load");
    assert_eq!(
        loaded.iter().map(|t| t.position).collect::<Vec<_>>(),
        vec![1, 2]
    );
    store.save_tabs(&[tab(7)]).await.expect("replace");
    let loaded = store.load_tabs().await.expect("load");
    assert_eq!(loaded.len(), 1);
    assert_eq!(
        loaded[0].table.as_ref().map(|t| t.name.as_str()),
        Some("User")
    );
}

#[tokio::test]
async fn reopening_a_file_keeps_data() {
    let dir = std::env::temp_dir().join(format!("ferrumdata-test-{}", EditorTabId::new()));
    std::fs::create_dir_all(&dir).expect("mkdir");
    let path = dir.join("state.db");
    {
        let store = AppStore::open(&path).await.expect("open");
        store
            .upsert_profile(&ConnectionProfile::new_postgres("p"))
            .await
            .expect("insert");
    }
    let store = AppStore::open(&path)
        .await
        .expect("reopen runs migrations idempotently");
    assert_eq!(store.list_profiles().await.expect("list").len(), 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// A row written by FerrumData before MySQL/SQLite/SurrealDB/MongoDB support
/// (no `namespace`, `favorite`, `folder`) must still load, next to new engines.
#[tokio::test]
async fn legacy_postgres_rows_load_alongside_new_engines() {
    let store = AppStore::in_memory().await.expect("open");
    let legacy = r#"{"id":"6f0e8a52-58b7-4d2c-9a43-3f1f3c0f7b11","display_name":"Old PG","engine":"Postgres","endpoint":{"Tcp":{"host":"db","port":5432}},"database":"app","username":"postgres","tls":"VerifyFull","read_only":true,"env":"Production","connect_timeout_secs":10,"secret_ref":null}"#;
    sqlx::query(
        "INSERT INTO connection_profiles (id, data, created_at, updated_at) VALUES (?, ?, ?, ?)",
    )
    .bind("6f0e8a52-58b7-4d2c-9a43-3f1f3c0f7b11")
    .bind(legacy)
    .bind("2024-01-01T00:00:00Z")
    .bind("2024-01-01T00:00:00Z")
    .execute(&store.pool)
    .await
    .expect("insert legacy row");
    let mongo = ConnectionProfile::new(ferrumdata_domain::DatabaseEngine::MongoDb, "docs");
    store.upsert_profile(&mongo).await.expect("insert new");

    let all = store.list_profiles().await.expect("list");
    assert_eq!(all.len(), 2, "{all:?}");
    let old = &all[0];
    assert_eq!(old.display_name, "Old PG");
    assert_eq!(old.engine, ferrumdata_domain::DatabaseEngine::Postgres);
    assert!(old.read_only);
    assert_eq!(old.namespace, None);
    assert_eq!(all[1], mongo);
}

fn history(profile: ProfileId, sql: &str) -> ferrumdata_domain::HistoryEntry {
    ferrumdata_domain::HistoryEntry {
        id: 0,
        profile_id: profile,
        sql: sql.into(),
        executed_at: Utc::now(),
        duration: std::time::Duration::from_millis(42),
        status: ferrumdata_domain::HistoryStatus::Completed,
        rows_returned: Some(7),
        rows_affected: None,
        error: None,
    }
}

fn all_of(profile: Option<ProfileId>, search: &str) -> HistoryQuery {
    HistoryQuery {
        profile_id: profile,
        search: search.into(),
        limit: 100,
    }
}

#[tokio::test]
async fn history_round_trips_newest_first_per_profile() {
    let store = AppStore::in_memory().await.expect("open");
    let (a, b) = (ProfileId::new(), ProfileId::new());
    store
        .record_history(&history(a, "select 1"))
        .await
        .expect("rec");
    let mut failed = history(a, "selec 2");
    failed.status = ferrumdata_domain::HistoryStatus::Failed;
    failed.rows_returned = None;
    failed.error = Some("syntax error at or near \"selec\"".into());
    let id = store.record_history(&failed).await.expect("rec");
    store
        .record_history(&history(b, "select 3"))
        .await
        .expect("rec");

    let got = store
        .list_history(&all_of(Some(a), ""))
        .await
        .expect("list");
    assert_eq!(
        got.iter().map(|e| e.sql.as_str()).collect::<Vec<_>>(),
        vec!["selec 2", "select 1"]
    );
    let mut expected = failed.clone();
    expected.id = id;
    // RFC 3339 keeps sub-second precision, so the entry survives unchanged.
    assert_eq!(got[0], expected);
    assert_eq!(got[1].duration, std::time::Duration::from_millis(42));
    assert_eq!(got[1].rows_returned, Some(7));

    assert_eq!(
        store
            .list_history(&all_of(None, ""))
            .await
            .expect("all")
            .len(),
        3
    );
}

#[tokio::test]
async fn history_search_is_literal_and_case_insensitive() {
    let store = AppStore::in_memory().await.expect("open");
    let p = ProfileId::new();
    for sql in [
        "SELECT * FROM Users",
        "select discount from t where pct = '50%'",
        "select 500 from t",
        "select a_b from t",
        "select axb from t",
    ] {
        store.record_history(&history(p, sql)).await.expect("rec");
    }
    let search = |s: &'static str| {
        let store = store.clone();
        async move {
            store
                .list_history(&all_of(Some(p), s))
                .await
                .expect("list")
                .into_iter()
                .map(|e| e.sql)
                .collect::<Vec<_>>()
        }
    };
    assert_eq!(search("from users").await, vec!["SELECT * FROM Users"]);
    assert_eq!(
        search("50%").await,
        vec!["select discount from t where pct = '50%'"]
    );
    assert_eq!(search("a_b").await, vec!["select a_b from t"]);
    assert_eq!(search("").await.len(), 5);
    let limited = store
        .list_history(&HistoryQuery {
            limit: 2,
            ..all_of(Some(p), "")
        })
        .await
        .expect("list");
    assert_eq!(limited.len(), 2);
}

#[tokio::test]
async fn history_is_pruned_cleared_and_removed_with_its_profile() {
    let store = AppStore::in_memory().await.expect("open");
    let p = ConnectionProfile::new_postgres("p");
    store.upsert_profile(&p).await.expect("profile");
    let other = ProfileId::new();
    for i in 0..HISTORY_RETENTION + 3 {
        store
            .record_history(&history(p.id, &format!("select {i}")))
            .await
            .expect("rec");
    }
    store
        .record_history(&history(other, "keep me"))
        .await
        .expect("rec");
    let all = HistoryQuery {
        limit: usize::MAX,
        ..all_of(Some(p.id), "")
    };
    let kept = store.list_history(&all).await.expect("list");
    assert_eq!(kept.len(), HISTORY_RETENTION);
    assert_eq!(kept[0].sql, format!("select {}", HISTORY_RETENTION + 2));
    assert_eq!(kept.last().map(|e| e.sql.as_str()), Some("select 3"));

    store.delete_profile(p.id).await.expect("delete");
    assert!(store.list_history(&all).await.expect("list").is_empty());
    assert_eq!(
        store
            .list_history(&all_of(None, ""))
            .await
            .expect("list")
            .len(),
        1
    );

    store.clear_history(Some(other)).await.expect("clear");
    assert!(
        store
            .list_history(&all_of(None, ""))
            .await
            .expect("list")
            .is_empty()
    );
}

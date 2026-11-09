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

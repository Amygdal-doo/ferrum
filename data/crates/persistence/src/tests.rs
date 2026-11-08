use super::*;
use ferrum_domain::{EditorTabId, SecretRef, Theme};

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
        table: (pos == 7).then(|| ferrum_domain::TableRef {
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
    let dir = std::env::temp_dir().join(format!("ferrum-test-{}", EditorTabId::new()));
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

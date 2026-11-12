use crate::Store;
use crate::test_util::{TempDb, exec, mem};
use ferrumapi_domain::{Collection, Folder, HttpMethod, HttpRequest, Item, SavedRequest};

fn nested(name: &str) -> Collection {
    let mut c = Collection::new(name);
    let mut inner = Folder::new("inner");
    inner.items.push(Item::Request(SavedRequest::new(
        "deep",
        HttpRequest::new(HttpMethod::Post, "https://example.test/{{path}}"),
    )));
    let mut outer = Folder::new("outer");
    outer.items.push(Item::Folder(inner));
    c.items.push(Item::Folder(outer));
    c.items.push(Item::Request(SavedRequest::new(
        "top",
        HttpRequest::new(HttpMethod::Get, "https://example.test"),
    )));
    c
}

#[tokio::test]
async fn nested_round_trip_update_delete() {
    let store = mem().await;
    assert!(store.list_collections().await.expect("list").is_empty());
    let mut c = nested("Alpha");
    store.save_collection(&c).await.expect("save");
    assert_eq!(
        store.list_collections().await.expect("list"),
        vec![c.clone()]
    );

    c.name = "Renamed".into();
    c.items.pop();
    store.save_collection(&c).await.expect("upsert");
    assert_eq!(
        store.list_collections().await.expect("list"),
        vec![c.clone()]
    );

    store.delete_collection(c.id).await.expect("delete");
    assert!(store.list_collections().await.expect("list").is_empty());
}

#[tokio::test]
async fn ordered_by_name_case_insensitive() {
    let store = mem().await;
    for name in ["charlie", "Bravo", "alpha", "Delta"] {
        store
            .save_collection(&Collection::new(name))
            .await
            .expect("save");
    }
    let names: Vec<String> = store
        .list_collections()
        .await
        .expect("list")
        .into_iter()
        .map(|c| c.name)
        .collect();
    assert_eq!(names, ["alpha", "Bravo", "charlie", "Delta"]);
}

#[tokio::test]
async fn corrupt_row_is_skipped() {
    let store = mem().await;
    let good = nested("Good");
    store.save_collection(&good).await.expect("save");
    exec(
        &store,
        "INSERT INTO collections (id, name, data, created_at, updated_at)
         VALUES ('bad', 'Aaa', '{not json', '', '')",
    )
    .await;
    assert_eq!(store.list_collections().await.expect("list"), vec![good]);
}

#[tokio::test]
async fn reopen_file_keeps_data_and_migrations_are_idempotent() {
    let db = TempDb::new();
    let c = nested("Persisted");
    {
        let store = Store::open(&db.path).await.expect("open");
        store.save_collection(&c).await.expect("save");
    }
    for _ in 0..2 {
        let store = Store::open(&db.path).await.expect("reopen");
        assert_eq!(
            store.list_collections().await.expect("list"),
            vec![c.clone()]
        );
    }
}

#[tokio::test]
async fn projects_and_mixed_kinds_round_trip() {
    use ferrumapi_domain::RequestKind;
    let store = mem().await;
    let mut c = nested("Mixed");
    c.project = Some("/code/app".into());
    let mut gql = HttpRequest::of_kind(RequestKind::Graphql);
    gql.specs.graphql.query = "{ me { id } }".into();
    let mut ws = HttpRequest::of_kind(RequestKind::WebSocket);
    ws.specs
        .websocket
        .templates
        .push(ferrumapi_domain::protocol::WsTemplate {
            name: "ping".into(),
            ..Default::default()
        });
    c.items.push(Item::Request(SavedRequest::new("gql", gql)));
    c.items.push(Item::Request(SavedRequest::new("ws", ws)));
    store.save_collection(&c).await.expect("save");
    let shared = Collection::new("Shared");
    store.save_collection(&shared).await.expect("save");
    let listed = store.list_collections().await.expect("list");
    assert_eq!(listed, vec![c.clone(), shared]);

    c.project = None;
    store.save_collection(&c).await.expect("move to shared");
    let listed = store.list_collections().await.expect("list");
    assert!(listed.iter().all(|c| c.project.is_none()));
}

/// A database created before migration 0004: its collections become
/// shared and its requests REST.
#[tokio::test]
async fn upgrading_an_old_database_keeps_collections_as_shared_rest() {
    let db = TempDb::new();
    let dir = std::env::temp_dir().join(format!("ferrumapi-mig-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&dir).expect("dir");
    for name in ["0001_initial.sql", "0002_cookies.sql", "0003_mocks.sql"] {
        let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("migrations")
            .join(name);
        std::fs::copy(src, dir.join(name)).expect("copy migration");
    }
    {
        let options = sqlx::sqlite::SqliteConnectOptions::new()
            .filename(&db.path)
            .create_if_missing(true);
        let pool = sqlx::SqlitePool::connect_with(options)
            .await
            .expect("old db");
        sqlx::migrate::Migrator::new(dir.as_path())
            .await
            .expect("old migrations")
            .run(&pool)
            .await
            .expect("run old");
        let old = r#"{"id":"6a1f1a43-5e8e-4a4e-9c2e-3b0f1f1b2c3d","name":"Legacy",
            "items":[{"kind":"request","id":"7b1f1a43-5e8e-4a4e-9c2e-3b0f1f1b2c3d","name":"get",
            "request":{"method":"POST","url":"https://x/a"}}]}"#;
        sqlx::query(
            "INSERT INTO collections (id, name, data, created_at, updated_at)
             VALUES ('6a1f1a43-5e8e-4a4e-9c2e-3b0f1f1b2c3d', 'Legacy', ?, '', '')",
        )
        .bind(old)
        .execute(&pool)
        .await
        .expect("insert old row");
        pool.close().await;
    }
    let _ = std::fs::remove_dir_all(&dir);
    let store = Store::open(&db.path).await.expect("upgrade");
    let listed = store.list_collections().await.expect("list");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].project, None);
    let Item::Request(r) = &listed[0].items[0] else {
        panic!("request")
    };
    assert_eq!(r.request.kind, ferrumapi_domain::RequestKind::Rest);
    assert_eq!(r.request.method, HttpMethod::Post);
}

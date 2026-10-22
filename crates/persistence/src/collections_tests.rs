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

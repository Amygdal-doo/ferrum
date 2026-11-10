use crate::Store;
use crate::test_util::{TempDb, exec, mem};
use ferrumapi_domain::mock::{CollectionMocks, MockResponse};
use ferrumapi_domain::{Collection, CollectionId, ItemId};

fn mocks() -> CollectionMocks {
    let mut ok = MockResponse::for_item(ItemId::new());
    ok.name = "ok".into();
    ok.delay_ms = 250;
    CollectionMocks {
        port: 4020,
        responses: vec![ok, MockResponse::for_item(ItemId::new())],
    }
}

async fn saved_collection(store: &Store) -> CollectionId {
    let c = Collection::new("API");
    store.save_collection(&c).await.expect("save collection");
    c.id
}

#[tokio::test]
async fn defaults_then_round_trip_and_replace() {
    let store = mem().await;
    let id = saved_collection(&store).await;
    assert_eq!(
        store.load_mocks(id).await.expect("load"),
        CollectionMocks::default()
    );
    let m = mocks();
    store.save_mocks(id, &m).await.expect("save");
    assert_eq!(store.load_mocks(id).await.expect("load"), m);
    let smaller = CollectionMocks::default();
    store.save_mocks(id, &smaller).await.expect("save");
    assert_eq!(store.load_mocks(id).await.expect("load"), smaller);
}

#[tokio::test]
async fn deleted_with_their_collection_and_saving_the_collection_keeps_them() {
    let store = mem().await;
    let id = saved_collection(&store).await;
    store.save_mocks(id, &mocks()).await.expect("save");
    // Re-saving the collection (an upsert) must not cascade.
    let mut c = Collection::new("Renamed");
    c.id = id;
    store.save_collection(&c).await.expect("resave");
    assert_eq!(store.load_mocks(id).await.expect("load").responses.len(), 2);
    store.delete_collection(id).await.expect("delete");
    assert_eq!(
        store.load_mocks(id).await.expect("load"),
        CollectionMocks::default()
    );
}

#[tokio::test]
async fn mocks_need_a_saved_collection() {
    let store = mem().await;
    assert!(
        store
            .save_mocks(CollectionId::new(), &mocks())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn unreadable_row_falls_back_to_defaults() {
    let store = mem().await;
    let c = Collection::new("X");
    store.save_collection(&c).await.expect("save");
    exec(&store, "INSERT INTO mocks (collection_id, data, updated_at) SELECT id, 'nope', '' FROM collections").await;
    assert_eq!(
        store.load_mocks(c.id).await.expect("load"),
        CollectionMocks::default()
    );
}

#[tokio::test]
async fn persists_across_reopen() {
    let db = TempDb::new();
    let id = {
        let store = Store::open(&db.path).await.expect("open");
        let id = saved_collection(&store).await;
        store.save_mocks(id, &mocks()).await.expect("save");
        id
    };
    let store = Store::open(&db.path).await.expect("reopen");
    assert_eq!(store.load_mocks(id).await.expect("load").port, 4020);
}

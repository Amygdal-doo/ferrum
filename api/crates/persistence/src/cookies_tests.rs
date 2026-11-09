use crate::Store;
use crate::test_util::{TempDb, exec, mem};
use chrono::Utc;
use ferrumapi_domain::{CookieJar, ResponseCookie};

fn jar() -> CookieJar {
    let mut jar = CookieJar::default();
    for (name, host) in [("a", "one.test"), ("b", "one.test"), ("c", "two.test")] {
        let cookie = ResponseCookie {
            name: name.into(),
            value: format!("v-{name}"),
            expires: Some("Max-Age=3600".into()),
            ..ResponseCookie::default()
        };
        assert!(jar.store(&cookie, host, "/", Utc::now()));
    }
    jar
}

#[tokio::test]
async fn empty_by_default_and_round_trips() {
    let store = mem().await;
    assert!(store.load_cookies().await.expect("load").is_empty());
    let jar = jar();
    store.save_cookies(&jar).await.expect("save");
    assert_eq!(store.load_cookies().await.expect("load"), jar);
}

#[tokio::test]
async fn saving_replaces_the_whole_jar() {
    let store = mem().await;
    store.save_cookies(&jar()).await.expect("save");
    let mut smaller = jar();
    smaller.clear_domain("one.test");
    store.save_cookies(&smaller).await.expect("save");
    let loaded = store.load_cookies().await.expect("load");
    assert_eq!(loaded.domains(), ["two.test"]);
    store
        .save_cookies(&CookieJar::default())
        .await
        .expect("clear");
    assert!(store.load_cookies().await.expect("load").is_empty());
}

#[tokio::test]
async fn unreadable_rows_are_skipped() {
    let store = mem().await;
    store.save_cookies(&jar()).await.expect("save");
    exec(
        &store,
        "INSERT INTO cookies (domain, path, name, data, updated_at)
         VALUES ('x.test', '/', 'bad', 'not json', '')",
    )
    .await;
    assert_eq!(store.load_cookies().await.expect("load").cookies().len(), 3);
}

#[tokio::test]
async fn persists_across_reopen() {
    let db = TempDb::new();
    {
        let store = Store::open(&db.path).await.expect("open");
        store.save_cookies(&jar()).await.expect("save");
    }
    let store = Store::open(&db.path).await.expect("reopen");
    assert_eq!(store.load_cookies().await.expect("load").cookies().len(), 3);
}

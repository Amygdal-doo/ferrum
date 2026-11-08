use crate::test_util::{exec, mem};
use chrono::{Duration, TimeZone, Utc};
use ferrumapi_domain::{HistoryEntry, HttpMethod, HttpRequest};

fn entry(minutes: i64) -> HistoryEntry {
    let mut e = HistoryEntry::new(
        HttpRequest::new(HttpMethod::Get, "https://example.test"),
        None,
    );
    let base = Utc
        .with_ymd_and_hms(2026, 1, 1, 0, 0, 0)
        .single()
        .expect("valid date");
    e.sent_at = base + Duration::minutes(minutes);
    e.status = Some(200);
    e
}

#[tokio::test]
async fn ordering_limit_and_pruning() {
    let store = mem().await;
    // Inserted out of order; listing must be newest first by sent_at.
    let entries: Vec<HistoryEntry> = [3, 1, 4, 0, 2].into_iter().map(entry).collect();
    for e in &entries {
        store.add_history(e, 100).await.expect("add");
    }
    let list = store.list_history(10).await.expect("list");
    let mins: Vec<i64> = list
        .iter()
        .map(|e| e.sent_at.timestamp() / 60 % 60)
        .collect();
    assert_eq!(mins, [4, 3, 2, 1, 0]);
    assert_eq!(list[0], entries[2]);

    assert_eq!(store.list_history(2).await.expect("list").len(), 2);

    // Adding with keep = 3 prunes to the 3 newest by sent_at.
    let newest = entry(10);
    store.add_history(&newest, 3).await.expect("add");
    let list = store.list_history(10).await.expect("list");
    let mins: Vec<i64> = list
        .iter()
        .map(|e| e.sent_at.timestamp() / 60 % 60)
        .collect();
    assert_eq!(mins, [10, 4, 3]);
}

#[tokio::test]
async fn delete_and_clear() {
    let store = mem().await;
    let a = entry(1);
    let b = entry(2);
    store.add_history(&a, 10).await.expect("add");
    store.add_history(&b, 10).await.expect("add");
    store.delete_history(b.id).await.expect("delete");
    assert_eq!(store.list_history(10).await.expect("list"), vec![a]);
    store.add_history(&b, 10).await.expect("add");
    store.clear_history().await.expect("clear");
    assert!(store.list_history(10).await.expect("list").is_empty());
}

#[tokio::test]
async fn corrupt_row_is_skipped() {
    let store = mem().await;
    let good = entry(1);
    store.add_history(&good, 10).await.expect("add");
    exec(
        &store,
        "INSERT INTO history (id, sent_at, sent_at_ms, data) VALUES ('bad', '', 9999999999999, 'nope')",
    )
    .await;
    assert_eq!(store.list_history(10).await.expect("list"), vec![good]);
}

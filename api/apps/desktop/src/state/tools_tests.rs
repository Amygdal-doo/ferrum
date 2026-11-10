use super::*;
use chrono::Utc;
use ferrumapi_domain::load::LoadSample;

#[test]
fn ports_and_statuses_are_validated() {
    assert_eq!(parse_port(" 4010 "), Ok(4010));
    assert!(parse_port("0").is_err());
    assert!(parse_port("70000").is_err());
    assert!(parse_port("abc").is_err());
    assert_eq!(parse_status("201"), Ok(201));
    assert!(parse_status("99").is_err());
    assert!(parse_status("600").is_err());
}

#[test]
fn mock_edits_mark_dirty_and_loading_does_not_clobber_them() {
    let mut s = MockState::new(CollectionId::new());
    let item = ItemId::new();
    let id = s.add_mock(item);
    let second = s.add_mock(item);
    assert!(s.dirty);
    assert_eq!(s.draft.responses[1].name, "Response 2");
    s.edit_mock(id, |m| m.status = 404);
    assert_eq!(s.draft.responses[0].status, 404);
    s.remove_mock(second);
    assert_eq!(s.draft.responses.len(), 1);

    s.set_loaded(CollectionMocks::default());
    assert!(s.loaded);
    assert_eq!(s.draft.responses.len(), 1, "unsaved edits kept");

    let mut fresh = MockState::new(CollectionId::new());
    fresh.set_loaded(CollectionMocks {
        port: 5000,
        responses: Vec::new(),
    });
    assert_eq!((fresh.port_text.as_str(), fresh.dirty), ("5000", false));
    fresh.set_port_text("bad".into());
    assert_eq!(fresh.draft.port, 5000);
    fresh.set_port_text("5001".into());
    assert_eq!(fresh.draft.port, 5001);
}

#[test]
fn mock_log_is_newest_first_and_capped() {
    let mut s = MockState::new(CollectionId::new());
    for i in 0..(MOCK_LOG_LIMIT + 5) {
        s.push_log(MockLogEntry {
            at: Utc::now(),
            method: "GET".into(),
            target: format!("/{i}"),
            status: 200,
            matched: None,
        });
    }
    assert_eq!(s.log.len(), MOCK_LOG_LIMIT);
    assert_eq!(s.log[0].target, format!("/{}", MOCK_LOG_LIMIT + 4));
}

#[test]
fn webhook_reply_and_selection() {
    let mut w = WebhookState::default();
    assert_eq!(w.reply(), Ok(WebhookReply::default()));
    w.status_text = "abc".into();
    assert!(w.reply().is_err());
    assert!(w.current().is_none());
    let capture = |id: u64| CapturedRequest {
        id,
        received_at: Utc::now(),
        method: "POST".into(),
        path: format!("/{id}"),
        query: String::new(),
        headers: Vec::new(),
        body: Vec::new(),
        size: 0,
    };
    w.captures = vec![capture(2), capture(1)];
    assert_eq!(w.current().map(|c| c.id), Some(2));
    w.selected = Some(1);
    assert_eq!(w.current().map(|c| c.id), Some(1));
    w.captures.truncate(1);
    w.refresh();
    assert_eq!(w.selected, None);
}

#[test]
fn load_test_run_lifecycle() {
    let mut l = LoadTestState::new(TabId::new(), "GET /".into(), None);
    l.total_text = "50000".into();
    assert!(l.begin(1, CancellationToken::new()).is_err());
    assert!(!l.running);
    l.total_text = "4".into();
    let config = l.begin(2, CancellationToken::new()).expect("valid");
    assert_eq!((config.total, config.concurrency), (4, 4));
    assert!(l.running);
    let mut tally = LoadTally::default();
    tally.record(LoadSample::Response {
        status: 200,
        latency_us: 10,
    });
    l.apply(LoadEvent::Progress(tally.clone()));
    assert!((l.progress() - 0.25).abs() < f32::EPSILON);
    l.stop();
    l.apply(LoadEvent::Done(tally, LoadEnd::Cancelled));
    assert!(!l.running && l.cancel.is_none());
    assert_eq!(l.end, Some(LoadEnd::Cancelled));
}

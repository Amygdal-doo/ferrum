use super::*;
use ferrumagent_ui::core::thread::{Thread, ThreadEvent};

fn summary(id: &str, title: &str, mode: Mode, hours_ago: i64, tokens: u64) -> ThreadSummary {
    ThreadSummary {
        id: id.into(),
        title: title.into(),
        agent: "test".into(),
        updated: Utc::now() - Duration::hours(hours_ago),
        messages: 2,
        mode,
        tokens,
    }
}

fn index() -> ThreadIndex {
    ThreadIndex {
        projects: vec![
            (
                PathBuf::from("/a"),
                vec![
                    summary("a1", "Fix login", Mode::Agent, 1, 1_000),
                    summary("a2", "Why is CI slow?", Mode::Ask, 24 * 10, 50_000),
                ],
            ),
            (
                PathBuf::from("/b"),
                vec![summary("b1", "Plan billing", Mode::Plan, 5, 0)],
            ),
        ],
    }
}

#[test]
fn recent_threads_and_discussions_span_projects() {
    let index = index();
    let ids: Vec<String> = index.recent(2).into_iter().map(|e| e.thread.id).collect();
    assert_eq!(ids, ["a1", "b1"]);
    let asks = index.with_mode(Mode::Ask);
    assert_eq!(asks.len(), 1);
    assert_eq!(asks[0].project, PathBuf::from("/a"));
    assert_eq!(index.of(Path::new("/b")).len(), 1);
    assert!(index.of(Path::new("/none")).is_empty());
}

#[test]
fn usage_totals_by_week_project_and_thread() {
    let usage = index().usage(Utc::now());
    assert_eq!((usage.total, usage.week), (51_000, 1_000));
    assert_eq!(
        usage.projects.len(),
        1,
        "projects without usage are left out"
    );
    assert_eq!(usage.projects[0].week, 1_000);
    let ids: Vec<&str> = usage.threads.iter().map(|e| e.thread.id.as_str()).collect();
    assert_eq!(ids, ["a2", "a1"]);
    assert_eq!(format_tokens(999), "999");
    assert_eq!(format_tokens(12_345), "12.3k");
    assert_eq!(format_tokens(4_100_000), "4.1M");
}

#[test]
fn search_matching_and_snippets() {
    assert!(matches("ci SLOW", "Why is CI slow?"));
    assert!(!matches("ci fast", "Why is CI slow?"));
    assert!(!matches("   ", "anything"));
    let text = "First line.\nThe build cache is cold, so cargo rebuilds everything each time.";
    let s = snippet(text, "cargo", 30).expect("snippet");
    assert!(
        s.contains("cargo") && s.starts_with('…') && s.ends_with('…'),
        "{s}"
    );
    assert_eq!(snippet("short", "short", 30).as_deref(), Some("short"));
    // Non-ASCII text before the match.
    let s = snippet("Größe ändern: Fenster", "fenster", 12).expect("snippet");
    assert!(s.contains("Fenster"), "{s}");
    assert_eq!(snippet(text, "absent", 30), None);
}

#[test]
fn search_reads_titles_and_messages_from_the_store() {
    let home = tempfile::tempdir().expect("tempdir");
    let project = PathBuf::from("/work/app");
    let store = ThreadStore::new(home.path(), &project);
    let mut a = Thread::new(project.clone(), "test", Mode::Agent);
    a.apply(&ThreadEvent::UserMessage {
        id: 1,
        text: "make the deploy idempotent".into(),
        attachments: vec![],
        checkpoint: None,
    });
    a.apply(&ThreadEvent::TitleChanged("Deploy script".into()));
    store.save(&a).expect("save");
    let mut b = Thread::new(project.clone(), "test", Mode::Ask);
    b.apply(&ThreadEvent::UserMessage {
        id: 1,
        text: "Where is the Postgres pool configured?".into(),
        attachments: vec![],
        checkpoint: None,
    });
    b.apply(&ThreadEvent::TitleChanged("Questions".into()));
    store.save(&b).expect("save");

    let index = ThreadIndex::load(home.path(), std::slice::from_ref(&project));
    assert_eq!(index.of(&project).len(), 2);
    let titles = |q: &str| -> Vec<String> {
        search(home.path(), &index, q, 10)
            .into_iter()
            .map(|h| h.entry.thread.title)
            .collect()
    };
    assert_eq!(titles("deploy"), ["Deploy script"]);
    assert_eq!(titles("postgres pool"), ["Questions"]);
    assert!(titles("kubernetes").is_empty());
    assert!(titles("").is_empty());
    let hit = &search(home.path(), &index, "postgres", 10)[0];
    assert!(hit.snippet.contains("Postgres"), "{}", hit.snippet);
}

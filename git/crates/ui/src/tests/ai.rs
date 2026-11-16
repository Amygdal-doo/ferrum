//! "Draft with AI" fills the commit box from the staged diff (a scripted
//! model answers) and commits nothing; the message stays editable.

use super::{git, host, wait_for};
use crate::{AppServices, GitView, GitViewOptions};
use ferrumagent_core::models::ModelStore;
use ferrumagent_core::provider::mock::MockProvider;
use ferrumagent_core::secrets::MemoryStore;
use ferrumagent_core::settings::AgentSettings;
use std::process::Command;
use std::sync::{Arc, Mutex};

fn head(dir: &std::path::Path) -> String {
    let out = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dir)
        .output()
        .expect("rev-parse");
    String::from_utf8_lossy(&out.stdout).trim().to_owned()
}

#[gpui::test]
fn drafts_the_commit_message_without_committing(cx: &mut gpui::TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = tempfile::tempdir().expect("repo");
    let dir = repo.path();
    git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("a.txt"), "one\n").expect("write");
    git(dir, &["add", "."]);
    git(dir, &["commit", "-q", "-m", "first"]);
    std::fs::write(dir.join("a.txt"), "one\ntwo\n").expect("write");
    git(dir, &["add", "."]);
    let before = head(dir);

    let mock = MockProvider::new();
    mock.push_text("Add the second line\n\nNeeded for the demo.");
    let store = ModelStore::new(
        data.path().join("agent"),
        Arc::new(MemoryStore::default()),
        Arc::new(Mutex::new(AgentSettings::default())),
        Some(Arc::new(mock.clone())),
        Vec::new(),
    );
    cx.executor().allow_parking();
    let services = AppServices::start(data.path())
        .expect("services")
        .with_model_store(store);
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init_with_services(services, cx);
    });
    let (host, cx) = host(Some(dir.to_path_buf()), GitViewOptions::desktop(), cx);
    let view = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &view, |v: &GitView| {
        v.state
            .active
            .is_some_and(|id| v.state.view(id).data.is_some())
    });

    view.update(cx, |v, cx| {
        let id = v.state.active.expect("active");
        v.draft_commit_message(id, cx);
    });
    wait_for(cx, &view, |v| {
        v.state
            .active
            .is_some_and(|id| !v.state.view(id).drafting && !v.state.view(id).summary.is_empty())
    });
    view.read_with(cx, |v, cx| {
        let id = v.state.active.expect("active");
        let r = v.state.view(id);
        assert_eq!(r.summary, "Add the second line");
        assert_eq!(r.description, "Needed for the demo.");
        // The commit box shows the draft, ready to edit.
        assert_eq!(v.inputs.summary.read(cx).text(), "Add the second line");
    });
    assert_eq!(head(dir), before, "drafting never commits");
    assert_eq!(mock.requests().len(), 1);
}

//! Headless GPUI tests of the embeddable view: a `GitView` on a temporary
//! repository, laid out at pane widths, with its events.

use crate::state::Page;
use crate::{
    AppServices, GitServices, GitView, GitViewEvent, GitViewOptions, WIDE_LAYOUT_MIN_WIDTH,
};
use gpui::{
    Context, Entity, IntoElement, Render, Subscription, TestAppContext, VisualTestContext, Window,
    div, prelude::*, px, size,
};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

/// AI drafts of the commit message.
mod ai;
/// Bisect from the commit menu.
mod bisect;
/// The three-way conflict view.
mod conflicts;
/// The stand-alone app's file editor.
mod editor;
/// The interactive rebase dialog.
mod rebase;

/// A host app's pane: the view plus the events it received.
pub(crate) struct Host {
    pub(crate) git: Entity<GitView>,
    pub(crate) events: Vec<GitViewEvent>,
    _events: Subscription,
}

impl Render for Host {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        div().size_full().child(self.git.clone())
    }
}

pub(crate) fn git(dir: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.name=Test", "-c", "user.email=test@example.com"])
        .args(args)
        .current_dir(dir)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// A repository with one commit and one uncommitted change.
fn repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("temp repo");
    git(dir.path(), &["init", "-q", "-b", "main"]);
    std::fs::write(dir.path().join("a.txt"), "one\n").expect("write");
    git(dir.path(), &["add", "a.txt"]);
    git(dir.path(), &["commit", "-q", "-m", "first"]);
    std::fs::write(dir.path().join("a.txt"), "one\ntwo\n").expect("write");
    dir
}

pub(crate) fn init(data: &Path, cx: &mut TestAppContext) {
    // Git and SQLite run on the services' own Tokio threads.
    cx.executor().allow_parking();
    let services = AppServices::start(data).expect("services");
    cx.update(|cx| {
        ferrum_ui::init_for_tests(cx);
        crate::init_with_services(services, cx);
    });
}

pub(crate) fn host(
    repo: Option<PathBuf>,
    options: GitViewOptions,
    cx: &mut TestAppContext,
) -> (Entity<Host>, &mut VisualTestContext) {
    cx.add_window_view(move |window, cx| {
        let git = GitView::new(repo, options, window, cx);
        let events = cx.subscribe(&git, |host: &mut Host, _, event: &GitViewEvent, _| {
            host.events.push(event.clone());
        });
        Host {
            git,
            events: Vec::new(),
            _events: events,
        }
    })
}

/// Lets the services runtime (real threads) finish until `done` holds.
pub(crate) fn wait_for(
    cx: &mut VisualTestContext,
    git: &Entity<GitView>,
    done: impl Fn(&GitView) -> bool,
) {
    for _ in 0..500 {
        cx.run_until_parked();
        if git.read_with(cx, |view, _| done(view)) {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("timed out waiting for the view");
}

pub(crate) fn canonical(path: &Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf())
}

#[gpui::test]
fn embedded_compact_view_renders_a_repository(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = repo();
    init(data.path(), cx);
    let (host, cx) = host(
        Some(repo.path().to_path_buf()),
        GitViewOptions::embedded(),
        cx,
    );
    let git = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &git, |v| {
        v.state
            .active
            .is_some_and(|id| v.state.view(id).data.is_some())
    });

    // Narrow pane: list over diff.
    cx.simulate_resize(size(px(360.), px(720.)));
    cx.run_until_parked();
    git.read_with(cx, |view, _| {
        let width = view.width.get().expect("laid out");
        assert!(f32::from(width) <= 360.0);
        assert!(!crate::components::shell::layout::is_wide(width));
        let id = view.state.active.expect("active repository");
        assert_eq!(view.state.open, vec![id], "one repository, no tabs");
        let data = view.state.view(id).data.expect("loaded");
        assert_eq!(data.snapshot.status.files.len(), 1);
        assert_eq!(data.snapshot.commits.len(), 1);
        assert_eq!(
            canonical(&view.repo_path().expect("path")),
            canonical(repo.path())
        );
    });

    // Every page renders in the narrow layout; the branch drawer opens.
    for page in [Page::WorkingCopy, Page::Stashes, Page::History] {
        git.update(cx, |view, cx| {
            let id = view.state.active.expect("active");
            view.set_page(id, page, cx);
            view.drawer_open = page == Page::History;
        });
        cx.run_until_parked();
    }

    // Wide pane: list and diff side by side.
    cx.simulate_resize(size(px(WIDE_LAYOUT_MIN_WIDTH + 200.), px(720.)));
    cx.run_until_parked();
    git.read_with(cx, |view, _| {
        let width = view.width.get().expect("laid out");
        assert!(crate::components::shell::layout::is_wide(width));
    });

    // The host hears about the repository and about files to open.
    git.update(cx, |view, cx| {
        let id = view.state.active.expect("active");
        view.open_in_host(id, "a.txt", Some(2), cx);
    });
    cx.run_until_parked();
    host.read_with(cx, |host, _| {
        let changed = host.events.iter().find_map(|e| match e {
            GitViewEvent::RepoChanged { path } => path.clone(),
            _ => None,
        });
        assert_eq!(changed.map(|p| canonical(&p)), Some(canonical(repo.path())));
        let opened = host.events.iter().find_map(|e| match e {
            GitViewEvent::OpenFile { path, line } => Some((canonical(path), *line)),
            _ => None,
        });
        assert_eq!(
            opened,
            Some((canonical(&repo.path().join("a.txt")), Some(2)))
        );
    });
}

#[gpui::test]
fn commits_are_reported_and_set_repo_switches(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let (first, second) = (repo(), repo());
    init(data.path(), cx);
    let (host, cx) = host(
        Some(first.path().to_path_buf()),
        GitViewOptions::embedded(),
        cx,
    );
    let git = host.read_with(cx, |h, _| h.git.clone());
    let loaded = |v: &GitView| {
        v.state
            .active
            .is_some_and(|id| v.state.view(id).data.is_some() && !v.state.is_busy(id))
    };
    wait_for(cx, &git, loaded);

    git.update(cx, |view, cx| {
        let id = view.state.active.expect("active");
        view.op(
            id,
            ferrumgit_domain::RepoOp::Stage(vec!["a.txt".into()]),
            cx,
        );
    });
    wait_for(cx, &git, |v| {
        loaded(v)
            && v.state
                .active
                .and_then(|id| v.state.view(id).data)
                .is_some_and(|d| d.snapshot.status.staged().len() == 1)
    });
    git.update(cx, |view, cx| {
        let id = view.state.active.expect("active");
        view.state.update_view(id, |v| v.summary = "second".into());
        assert!(view.can_commit(id));
        view.commit(id, cx);
    });
    wait_for(cx, &git, |v| {
        loaded(v)
            && v.state
                .active
                .and_then(|id| v.state.view(id).data)
                .is_some_and(|d| d.snapshot.commits.len() == 2)
    });
    host.read_with(cx, |host, _| {
        assert!(host.events.iter().any(|e| matches!(
            e,
            GitViewEvent::CommitCreated { summary, amend: false, .. } if summary == "second"
        )));
    });

    // The host follows its project: one repository replaces the other.
    git.update(cx, |view, cx| view.set_repo(second.path(), cx));
    wait_for(cx, &git, |v| {
        loaded(v)
            && v.repo_path()
                .is_some_and(|p| canonical(&p) == canonical(second.path()))
    });
    git.read_with(cx, |view, _| assert_eq!(view.state.open.len(), 1));
}

#[gpui::test]
fn desktop_view_shares_the_process_services(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    init(data.path(), cx);
    // Initializing again (as a second host crate might) changes nothing.
    cx.update(crate::init);
    let (host, cx) = host(None, GitViewOptions::desktop(), cx);
    let git = host.read_with(cx, |h, _| h.git.clone());
    wait_for(cx, &git, |v| v.state.loaded);
    git.read_with(cx, |view, cx| {
        assert_eq!(view.state.active, None, "home screen");
        assert!(view.options.show_repo_tabs);
        assert!(cx.has_global::<GitServices>());
    });
    // A second view on the same services.
    let second = cx.update(|window, cx| GitView::new(None, GitViewOptions::embedded(), window, cx));
    wait_for(cx, &second, |v| v.state.loaded);
}

#[gpui::test]
fn a_host_without_the_toolbar_drives_the_repository(cx: &mut TestAppContext) {
    let data = tempfile::tempdir().expect("data dir");
    let repo = repo();
    init(data.path(), cx);
    let options = GitViewOptions::embedded().without_toolbar();
    assert!(!options.show_toolbar && GitViewOptions::embedded().show_toolbar);
    let (host, cx) = host(Some(repo.path().to_path_buf()), options, cx);
    let git = host.read_with(cx, |h, _| h.git.clone());
    assert_eq!(git.read_with(cx, |v, _| v.summary()), None);
    wait_for(cx, &git, |v| v.summary().is_some());

    // The head for the host's status line.
    let summary = git.read_with(cx, |v, _| v.summary()).expect("summary");
    assert_eq!(summary.branch.as_deref(), Some("main"));
    assert_eq!((summary.ahead, summary.behind), (0, 0));
    assert_eq!(summary.changed, 1);
    assert_eq!(summary.busy, None);

    // The branch drawer and the commit form open on request.
    git.update(cx, |view, cx| view.show_branches(cx));
    cx.run_until_parked();
    assert!(git.read_with(cx, |v, _| v.drawer_open));
    cx.update(|window, cx| git.update(cx, |view, cx| view.start_commit(window, cx)));
    cx.run_until_parked();
    git.read_with(cx, |view, _| {
        let id = view.state.active.expect("active");
        assert_eq!(view.state.view(id).page, Page::WorkingCopy);
    });

    // Fetch without a remote fails without wedging the view.
    git.update(cx, |view, cx| view.remote(crate::RemoteCommand::Fetch, cx));
    wait_for(cx, &git, |v| v.summary().is_some_and(|s| s.busy.is_none()));
}

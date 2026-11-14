//! Headless GPUI tests of the whole window: the real views (chats on an
//! isolated config, git on temporary services, terminals on a private
//! in-process server), drawn and driven without showing a window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ferrumagent_ui::core::Mode;
use ferrumagent_ui::core::session::SessionConfig;
use ferrumagent_ui::core::store::ThreadStore;
use ferrumagent_ui::core::thread::{Thread, ThreadEvent};
use ferrumagent_ui::{ChatEvent, ChatOptions, GitRequest};
use ferrumgit_ui::GitViewEvent;
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size};
use tempfile::TempDir;

use crate::app::{FerrumVibe, Options};
use crate::automations::{Automation, Schedule};
use crate::layout::{self, Column, Layout};
use crate::nav::Page;

type Opened = Rc<RefCell<Vec<(Option<PathBuf>, PathBuf, Option<u32>)>>>;

struct Fixture {
    dir: TempDir,
    opened: Opened,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        for sub in ["a", "b", "home", "git-data", "data"] {
            std::fs::create_dir_all(dir.path().join(sub)).expect("mkdir");
        }
        Self {
            dir,
            opened: Rc::default(),
        }
    }

    fn path(&self, sub: &str) -> PathBuf {
        self.dir.path().join(sub).canonicalize().expect("canonical")
    }

    fn layouts_file(&self) -> PathBuf {
        self.dir.path().join("layouts.json")
    }

    /// A saved thread of `project` in the chats' home.
    fn thread(&self, project: &Path, title: &str, text: &str, mode: Mode) -> String {
        let mut thread = Thread::new(project.to_path_buf(), "test", mode);
        thread.apply(&ThreadEvent::UserMessage {
            id: 1,
            text: text.into(),
            attachments: vec![],
            checkpoint: None,
        });
        thread.apply(&ThreadEvent::TitleChanged(title.into()));
        ThreadStore::new(&self.path("home"), project)
            .save(&thread)
            .expect("save thread");
        thread.id
    }

    fn options(&self, project: Option<PathBuf>) -> Options {
        let home = self.path("home");
        let opened = self.opened.clone();
        Options {
            project,
            layouts_file: Some(self.layouts_file()),
            projects_file: Some(self.dir.path().join("projects.json")),
            data_dir: Some(self.path("data")),
            agent_home: home.clone(),
            chat_options: Rc::new(move |root: &Path| ChatOptions {
                config: Some(SessionConfig::isolated(root, &home)),
                ..Default::default()
            }),
            term_attach_to_server: false,
            open_file: Rc::new(move |project: Option<&Path>, file: &Path, line| {
                opened.borrow_mut().push((
                    project.map(Path::to_path_buf),
                    file.to_path_buf(),
                    line,
                ));
                Ok(())
            }),
        }
    }

    fn open<'a>(
        &self,
        cx: &'a mut TestAppContext,
        project: Option<PathBuf>,
    ) -> (Entity<FerrumVibe>, &'a mut VisualTestContext) {
        cx.executor().allow_parking();
        let git_data = self.path("git-data");
        cx.update(|cx| {
            ferrum_ui::init_for_tests(cx);
            ferrumagent_ui::init(cx);
            let services = ferrumgit_ui::AppServices::start(&git_data).expect("git services");
            ferrumgit_ui::init_with_services(services, cx);
            ferrumterm_ui::init(cx);
            crate::app::bind_keys(cx);
        });
        let options = self.options(project);
        let (app, cx) = cx.add_window_view(|window, cx| FerrumVibe::new(options, window, cx));
        cx.simulate_resize(size(px(1500.), px(900.)));
        draw(cx);
        (app, cx)
    }
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn read<R>(
    app: &Entity<FerrumVibe>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&FerrumVibe) -> R,
) -> R {
    app.read_with(cx, |app, _| f(app))
}

fn update(
    app: &Entity<FerrumVibe>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&mut FerrumVibe, &mut gpui::Window, &mut gpui::Context<FerrumVibe>),
) {
    cx.update(|window, cx| app.update(cx, |app, cx| f(app, window, cx)));
    draw(cx);
}

/// Panes drawn on screen, left to right.
fn drawn(cx: &mut VisualTestContext) -> Vec<Column> {
    let mut shown: Vec<(f32, Column)> = Column::ALL
        .into_iter()
        .filter_map(|c| {
            let key: &'static str = match c {
                Column::Chat => "column-chat",
                Column::Git => "column-git",
                Column::Term => "column-term",
            };
            cx.debug_bounds(key).map(|b| (f32::from(b.left()), c))
        })
        .collect();
    shown.sort_by(|a, b| a.0.total_cmp(&b.0));
    shown.into_iter().map(|(_, c)| c).collect()
}

fn width(cx: &mut VisualTestContext, key: &'static str) -> f32 {
    f32::from(cx.debug_bounds(key).expect(key).size.width)
}

/// Waits (on real threads and timers) until `done` holds.
fn wait(app: &Entity<FerrumVibe>, cx: &mut VisualTestContext, done: impl Fn(&FerrumVibe) -> bool) {
    for _ in 0..400 {
        // Test timers only fire when the clock is moved.
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(50));
        draw(cx);
        if read(app, cx, &done) {
            return;
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    panic!("timed out");
}

#[gpui::test]
fn the_chat_is_always_shown_and_panes_toggle(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, Some(fx.path("a")));
    assert_eq!(drawn(cx), [Column::Chat]);
    assert!(
        cx.debug_bounds("nav-Search").is_some(),
        "the sidebar is drawn"
    );
    read(&app, cx, |a| {
        assert!(a.chat().is_some() && a.git.is_some() && a.term.is_none());
    });

    // ⌥⌘2 / ⌥⌘3 open Git and Term to the right of the chat.
    cx.simulate_keystrokes("cmd-alt-2");
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Git]);
    cx.simulate_keystrokes("cmd-alt-3");
    draw(cx);
    assert_eq!(drawn(cx), Column::ALL);
    assert!(read(&app, cx, |a| a.term.is_some()));
    // The chat can't be closed.
    update(&app, cx, |a, w, cx| {
        a.set_visible(Column::Chat, false, w, cx)
    });
    assert_eq!(drawn(cx), Column::ALL);
    cx.simulate_keystrokes("cmd-alt-2");
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Term]);

    // Narrow windows still lay out.
    cx.simulate_resize(size(px(760.), px(520.)));
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Term]);
}

#[gpui::test]
fn the_sidebar_folds_and_remembers(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, Some(fx.path("a")));
    let wide = width(cx, "column-chat");
    // Keys go to the window when nothing inside has focus.
    update(&app, cx, |a, w, cx| w.focus(a.focus_handle(), cx));
    cx.simulate_keystrokes("cmd-\\");
    draw(cx);
    assert!(cx.debug_bounds("nav-Search").is_none());
    assert!(width(cx, "column-chat") > wide + 150.);
    let saved = crate::nav::UiState::load(&fx.path("data").join("ui.json"));
    assert!(!saved.sidebar_open);
    cx.simulate_keystrokes("cmd-b");
    draw(cx);
    assert!(cx.debug_bounds("nav-Search").is_some());
}

#[gpui::test]
fn events_from_the_chat_and_git_are_wired(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = fx.path("a");
    let (app, cx) = fx.open(cx, Some(root.clone()));
    let (chat, git) = read(&app, cx, |a| {
        (a.chat().expect("chat"), a.git.clone().expect("git"))
    });

    // Files from the chat and from git open in the Editor.
    let file = root.join("src/main.rs");
    chat.update(cx, |_, cx| {
        cx.emit(ChatEvent::OpenFile {
            path: file.clone(),
            line: Some(7),
        });
    });
    git.update(cx, |_, cx| {
        cx.emit(GitViewEvent::OpenFile {
            path: file.clone(),
            line: None,
        });
    });
    assert_eq!(
        *fx.opened.borrow(),
        [
            (Some(root.clone()), file.clone(), Some(7)),
            (Some(root.clone()), file, None)
        ]
    );

    // Agent changes reload the git view.
    let before = read(&app, cx, |a| a.git_refreshes);
    chat.update(cx, |_, cx| cx.emit(ChatEvent::ChangesUpdated { files: 2 }));
    draw(cx);
    assert_eq!(
        read(&app, cx, |a| (a.pending_changes, a.git_refreshes)),
        (2, before + 1)
    );

    // "Run in terminal" opens the terminal pane.
    chat.update(cx, |_, cx| {
        cx.emit(ChatEvent::RunInTerminal {
            cmd: "echo hi".into(),
            cwd: root.clone(),
        });
    });
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Term]);

    // The footer's git actions: Commit opens the Git pane; the remote
    // ones run in the background.
    chat.update(cx, |_, cx| cx.emit(ChatEvent::Git(GitRequest::Commit)));
    draw(cx);
    assert_eq!(drawn(cx), Column::ALL);
    chat.update(cx, |_, cx| cx.emit(ChatEvent::Git(GitRequest::Fetch)));
    draw(cx);
}

#[gpui::test]
fn the_git_summary_reaches_the_chat(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = fx.path("a");
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(["-c", "user.name=T", "-c", "user.email=t@e.x"])
            .args(args)
            .current_dir(&root)
            .status()
            .expect("git")
            .success();
        assert!(ok, "git {args:?}");
    };
    git(&["init", "-q", "-b", "feat"]);
    std::fs::write(root.join("f.txt"), "1").expect("write");
    git(&["add", "."]);
    git(&["commit", "-q", "-m", "one"]);
    std::fs::write(root.join("f.txt"), "2").expect("write");
    let (app, cx) = fx.open(cx, Some(root.clone()));
    wait(&app, cx, |a| a.git_summary.is_some());
    let summary = read(&app, cx, |a| a.git_summary.clone()).expect("summary");
    assert_eq!(summary.branch.as_deref(), Some("feat"));
    assert_eq!(summary.changed, 1);
    let status = crate::panes::git_status(&summary);
    assert_eq!((status.branch.as_str(), status.dirty), ("feat", 1));
}

#[gpui::test]
fn pages_replace_the_chat_and_back_returns(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, Some(fx.path("a")));
    for page in [
        Page::Projects,
        Page::Discussions,
        Page::Search,
        Page::Knowledge,
        Page::Sites,
        Page::Automations,
        Page::Extensions,
        Page::Usage,
    ] {
        update(&app, cx, |a, w, cx| a.show_page(page, w, cx));
        // Debug selectors are `&'static str`: one leaked string per page.
        let key: &'static str = Box::leak(format!("page-{}", page.title()).into_boxed_str());
        assert!(cx.debug_bounds(key).is_some(), "{page:?}");
        assert_eq!(read(&app, cx, |a| a.page()), page);
    }
    // ⌘[ walks back through the pages to the chat; ⌘] forward again.
    update(&app, cx, |a, w, cx| w.focus(a.focus_handle(), cx));
    cx.simulate_keystrokes("cmd-[");
    draw(cx);
    assert_eq!(read(&app, cx, |a| a.page()), Page::Extensions);
    for _ in 0..7 {
        update(&app, cx, |a, w, cx| a.go_back(w, cx));
    }
    assert_eq!(read(&app, cx, |a| a.page()), Page::Chat);
    assert!(cx.debug_bounds("page-Projects").is_none());
    update(&app, cx, |a, w, cx| a.go_forward(w, cx));
    assert_eq!(read(&app, cx, |a| a.page()), Page::Projects);
}

#[gpui::test]
fn threads_are_listed_searched_and_opened_across_projects(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (a, b) = (fx.path("a"), fx.path("b"));
    let fix = fx.thread(&b, "Fix the deploy", "make deploy idempotent", Mode::Agent);
    let ask = fx.thread(&a, "Where is the pool?", "postgres pool size", Mode::Ask);
    // b is a recent project too.
    ferrum_ui::project::record(&fx.dir.path().join("projects.json"), &b, "ferrumvibe")
        .expect("record");
    let (app, cx) = fx.open(cx, Some(a.clone()));
    wait(&app, cx, |app| app.index.recent(8).len() == 2);
    let discussions = read(&app, cx, |app| app.index.with_mode(Mode::Ask));
    assert_eq!(discussions.len(), 1);
    assert_eq!(discussions[0].thread.id, ask);
    assert_eq!(
        read(&app, cx, |app| app.recent.clone()),
        [a.clone(), b.clone()]
    );

    // Search finds message text in another project.
    update(&app, cx, |app, w, cx| app.show_page(Page::Search, w, cx));
    let input = read(&app, cx, |app| app.pages.search.clone());
    // Typing runs the search; `set_text` (not typing) is followed by it.
    input.update(cx, |input, cx| input.set_text("idempotent", cx));
    update(&app, cx, |app, _, cx| app.run_search(cx));
    wait(&app, cx, |app| !app.pages.hits.is_empty());
    let hit = read(&app, cx, |app| app.pages.hits[0].clone());
    assert_eq!(
        (hit.entry.project.clone(), hit.entry.thread.id.clone()),
        (b.clone(), fix.clone())
    );

    // Opening it switches project; back returns to the search.
    update(&app, cx, |app, w, cx| app.open_thread(&b, &fix, w, cx));
    assert_eq!(
        read(&app, cx, |app| (app.project.clone(), app.page())),
        (Some(b.clone()), Page::Chat)
    );
    let chat_b = read(&app, cx, |app| app.chat().expect("chat"));
    update(&app, cx, |app, w, cx| app.go_back(w, cx));
    assert_eq!(read(&app, cx, |app| app.page()), Page::Search);
    // Each project keeps its own chat alive.
    update(&app, cx, |app, w, cx| app.set_project(a.clone(), w, cx));
    update(&app, cx, |app, w, cx| app.set_project(b.clone(), w, cx));
    assert_eq!(
        read(&app, cx, |app| app.chat().expect("chat").entity_id()),
        chat_b.entity_id()
    );
    assert_eq!(read(&app, cx, |app| app.chats.len()), 2);
}

#[gpui::test]
fn automations_run_now_and_on_schedule(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (a, b) = (fx.path("a"), fx.path("b"));
    let file = fx.path("data").join("automations.json");
    let mut due = Automation::new(
        "Nightly",
        &b,
        "update deps",
        Mode::Agent,
        Schedule::Every { hours: 1 },
    );
    due.created = chrono::Utc::now() - chrono::Duration::hours(2);
    let manual = Automation::new("Review", &a, "review the diff", Mode::Ask, Schedule::Manual);
    crate::automations::save(&file, &[due.clone(), manual.clone()]).expect("save");
    let (app, cx) = fx.open(cx, Some(a.clone()));

    // The due one starts in b's chat without leaving a.
    update(&app, cx, |app, w, cx| app.run_due(w, cx));
    read(&app, cx, |app| {
        assert_eq!(app.project.as_deref(), Some(a.as_path()));
        assert!(app.chats.contains_key(&b));
    });
    let saved = crate::automations::load(&file);
    assert!(saved[0].last_run.is_some() && saved[1].last_run.is_none());
    // Not due again right away.
    update(&app, cx, |app, w, cx| app.run_due(w, cx));
    assert_eq!(
        crate::automations::load(&file)[0].last_run,
        saved[0].last_run
    );

    // Run now shows the chat of its project.
    update(&app, cx, |app, w, cx| {
        app.show_page(Page::Automations, w, cx)
    });
    update(&app, cx, |app, w, cx| {
        app.run_automation(&manual.id, true, w, cx)
    });
    assert_eq!(read(&app, cx, |app| app.page()), Page::Chat);
    assert!(crate::automations::load(&file)[1].last_run.is_some());
}

#[gpui::test]
fn layouts_are_kept_per_project(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (a, b) = (fx.path("a"), fx.path("b"));
    let (app, cx) = fx.open(cx, Some(a.clone()));

    // Project a: git shown, chat wider.
    update(&app, cx, |app, w, cx| {
        app.set_visible(Column::Git, true, w, cx)
    });
    let layout_a = read(&app, cx, |app| app.layout.clone());
    assert_eq!(layout::load(&fx.layouts_file(), Some(&a)), layout_a);

    // Project b starts from the last layout; add the terminal.
    update(&app, cx, |app, w, cx| app.set_project(b.clone(), w, cx));
    assert_eq!(drawn(cx), [Column::Chat, Column::Git]);
    update(&app, cx, |app, w, cx| {
        app.set_visible(Column::Term, true, w, cx)
    });
    let layout_b = read(&app, cx, |app| app.layout.clone());
    assert_eq!(drawn(cx), Column::ALL);

    // Back to a: its own layout returns.
    update(&app, cx, |app, w, cx| app.set_project(a.clone(), w, cx));
    assert_eq!(drawn(cx), [Column::Chat, Column::Git]);
    assert_eq!(layout::load(&fx.layouts_file(), Some(&b)), layout_b);
    assert_ne!(layout_b, Layout::default());
}

#[gpui::test]
fn dragging_a_splitter_resizes_its_neighbours(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, Some(fx.path("a")));
    update(&app, cx, |app, w, cx| {
        app.set_visible(Column::Git, true, w, cx);
        app.set_visible(Column::Term, true, w, cx);
    });
    let (chat, git, term) = (
        width(cx, "column-chat"),
        width(cx, "column-git"),
        width(cx, "column-term"),
    );
    let start = cx.debug_bounds("splitter-chat").expect("splitter").center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    for step in 1..=4 {
        let at = point(start.x - px(40. * step as f32), start.y);
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    let end = point(start.x - px(160.), start.y);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    draw(cx);
    let (chat2, git2, term2) = (
        width(cx, "column-chat"),
        width(cx, "column-git"),
        width(cx, "column-term"),
    );
    assert!((chat - chat2 - 160.).abs() < 6., "{chat} → {chat2}");
    assert!((git2 - git - 160.).abs() < 6., "{git} → {git2}");
    assert!((term2 - term).abs() < 2., "{term} → {term2}");
}

#[gpui::test]
fn without_a_project_nothing_starts_until_one_opens(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, None);
    assert_eq!(drawn(cx), [Column::Chat]);
    read(&app, cx, |app| {
        assert!(app.chat().is_none() && app.git.is_none() && app.term.is_none());
    });
    update(&app, cx, |app, w, cx| app.open_project(fx.path("a"), w, cx));
    read(&app, cx, |app| {
        assert!(app.chat().is_some() && app.git.is_some())
    });
}

#[test]
fn the_folder_argument_skips_flags_and_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("f.txt");
    std::fs::write(&file, "x").expect("write");
    let args =
        |list: &[&std::ffi::OsStr]| list.iter().map(|a| a.to_os_string()).collect::<Vec<_>>();
    let real = dir.path().canonicalize().expect("canonical");
    assert_eq!(
        crate::folder_argument(args(&["-psn_0_1".as_ref(), dir.path().as_os_str()]).into_iter()),
        Some(real)
    );
    assert_eq!(
        crate::folder_argument(args(&[file.as_os_str()]).into_iter()),
        None
    );
    assert_eq!(crate::folder_argument(std::iter::empty()), None);
}

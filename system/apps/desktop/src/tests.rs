//! Headless GPUI tests of the whole window: the first-run demo, the empty
//! view after it, and the sidebar / panel toggles kept in the prefs file.
//! Prefs and the recent-projects list live in a temporary folder.

use std::path::PathBuf;

use ferrumsystem_core::{demos, store};
use gpui::{Entity, TestAppContext, VisualTestContext, px, size};
use tempfile::TempDir;

use crate::app::{FerrumSystem, NewDiagram, OpenDemo, Options, Source};
use crate::prefs::{self, UiPrefs};

pub(crate) struct Fixture {
    dir: TempDir,
}

impl Fixture {
    pub(crate) fn new() -> Self {
        Self {
            dir: tempfile::tempdir().expect("tempdir"),
        }
    }

    fn prefs_file(&self) -> PathBuf {
        self.dir.path().join("ferrumsystem").join("ui.json")
    }

    fn prefs(&self) -> UiPrefs {
        prefs::load(Some(&self.prefs_file()))
    }

    pub(crate) fn set_prefs(&self, prefs: &UiPrefs) {
        prefs::save(Some(&self.prefs_file()), prefs).expect("save prefs");
    }

    /// A project folder, with a starter diagram when `with_system`.
    pub(crate) fn project(&self, name: &str, with_system: bool) -> PathBuf {
        let path = self.dir.path().join(name);
        std::fs::create_dir_all(&path).expect("mkdir");
        if with_system {
            store::save(&path, &demos::starter(name)).expect("save system");
        }
        path
    }

    pub(crate) fn open<'a>(
        &self,
        cx: &'a mut TestAppContext,
        project: Option<PathBuf>,
    ) -> (Entity<FerrumSystem>, &'a mut VisualTestContext) {
        cx.update(|cx| {
            ferrum_ui::init_for_tests(cx);
            ferrumagent_ui::init(cx);
            crate::app::bind_keys(cx);
        });
        let options = Options {
            project,
            prefs_file: Some(self.prefs_file()),
            projects_file: Some(self.dir.path().join("projects.json")),
            agent_home: Some(self.dir.path().join("agent")),
        };
        let (app, cx) = cx.add_window_view(|window, cx| FerrumSystem::new(options, window, cx));
        cx.simulate_resize(size(px(1440.), px(880.)));
        draw(cx);
        (app, cx)
    }
}

pub(crate) fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

pub(crate) fn seen() -> UiPrefs {
    UiPrefs {
        demo_seen: true,
        ..UiPrefs::default()
    }
}

#[gpui::test]
fn the_first_start_shows_the_demo_once(cx: &mut TestAppContext) {
    let f = Fixture::new();
    let (app, cx) = f.open(cx, None);
    app.read_with(cx, |app, _| {
        assert_eq!(app.source, Source::Demo(0));
        assert_eq!(app.system.name, demos::banking().name);
    });
    assert!(
        f.prefs().demo_seen,
        "the first start records the demo as seen"
    );
}

#[gpui::test]
fn after_the_first_start_the_window_opens_empty(cx: &mut TestAppContext) {
    let f = Fixture::new();
    f.set_prefs(&seen());
    let (app, cx) = f.open(cx, None);
    app.read_with(cx, |app, _| {
        assert!(app.is_empty());
        assert!(app.system.nodes.is_empty());
    });
}

#[gpui::test]
fn a_project_without_a_diagram_opens_empty_once_the_demo_was_seen(cx: &mut TestAppContext) {
    let f = Fixture::new();
    f.set_prefs(&seen());
    let project = f.project("bare", false);
    let (app, cx) = f.open(cx, Some(project.clone()));
    app.read_with(cx, |app, _| {
        assert!(app.is_empty());
        assert_eq!(app.project.as_ref(), Some(&project));
    });
}

#[gpui::test]
fn the_last_project_is_reopened(cx: &mut TestAppContext) {
    let f = Fixture::new();
    let project = f.project("shop", true);
    f.set_prefs(&UiPrefs {
        last_project: Some(project.clone()),
        ..seen()
    });
    let (app, cx) = f.open(cx, None);
    app.read_with(cx, |app, _| {
        assert_eq!(app.source, Source::Project(project.clone()));
        assert_eq!(app.system.name, "shop");
    });
}

#[gpui::test]
fn opening_a_project_remembers_it(cx: &mut TestAppContext) {
    let f = Fixture::new();
    let project = f.project("api", true);
    let (_app, _cx) = f.open(cx, Some(project.clone()));
    let prefs = f.prefs();
    assert_eq!(prefs.last_project, Some(project));
    assert!(!prefs.demo_seen, "a project diagram is shown, not the demo");
}

#[gpui::test]
fn the_demo_opens_from_the_menu_at_any_time(cx: &mut TestAppContext) {
    let f = Fixture::new();
    f.set_prefs(&seen());
    let (app, cx) = f.open(cx, None);
    cx.dispatch_action(OpenDemo { index: 1 });
    draw(cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.source, Source::Demo(1));
        assert_eq!(app.system.name, demos::rag().name);
    });
}

#[gpui::test]
fn new_diagram_starts_the_open_projects_architecture(cx: &mut TestAppContext) {
    let f = Fixture::new();
    f.set_prefs(&seen());
    let project = f.project("bare", false);
    let (app, cx) = f.open(cx, Some(project.clone()));
    cx.dispatch_action(NewDiagram);
    draw(cx);
    app.read_with(cx, |app, _| {
        assert_eq!(app.source, Source::Project(project.clone()));
        assert!(app.project_has_system);
    });
    assert!(store::system_file(&project).is_file());
}

#[gpui::test]
fn the_panel_and_sidebar_fold_with_their_shortcuts_and_stay_folded(cx: &mut TestAppContext) {
    let f = Fixture::new();
    let project = f.project("shop", true);
    {
        let (app, cx) = f.open(cx, Some(project.clone()));
        app.read_with(cx, |app, _| {
            assert!(app.panel_open() && app.sidebar_open());
        });
        cx.simulate_keystrokes("alt-cmd-b");
        draw(cx);
        app.read_with(cx, |app, _| assert!(!app.panel_open()));
        cx.simulate_keystrokes("cmd-b");
        draw(cx);
        app.read_with(cx, |app, _| assert!(!app.sidebar_open()));
    }
    let prefs = f.prefs();
    assert!(!prefs.panel_open && !prefs.sidebar_open);

    // A new window starts with both still folded.
    let (app, cx) = f.open(cx, None);
    app.read_with(cx, |app, _| {
        assert!(!app.panel_open() && !app.sidebar_open());
        assert_eq!(app.source, Source::Project(project.clone()));
    });
    cx.simulate_keystrokes("alt-cmd-b");
    draw(cx);
    app.read_with(cx, |app, _| assert!(app.panel_open()));
    assert!(f.prefs().panel_open);
}

/// A project whose `system.json` is hand-written: two-space indent, a
/// field FerrumSystem does not know, and no `slot`s.
fn hand_written(f: &Fixture) -> (PathBuf, String) {
    let project = f.project("hand", false);
    let text = r#"{
  "name": "Hand",
  "x-owner": "platform",
  "layers": [
    { "id": "services", "name": "Services", "color": 2278750 },
    { "id": "data", "name": "Data", "color": 15485081 }
  ],
  "nodes": [
    { "id": "api", "name": "API", "kind": "service", "layer": "services", "x-sla": "99.9" },
    { "id": "db", "name": "DB", "kind": "sql-db", "layer": "data" }
  ],
  "edges": [{ "from": "api", "to": "db", "style": "request" }]
}
"#;
    let file = store::system_file(&project);
    std::fs::create_dir_all(file.parent().expect("dir")).expect("mkdir");
    std::fs::write(&file, text).expect("write");
    (project, text.to_owned())
}

#[gpui::test]
fn edits_keep_the_files_formatting_and_undo_and_redo(cx: &mut TestAppContext) {
    let f = Fixture::new();
    f.set_prefs(&seen());
    let (project, text) = hand_written(&f);
    let file = store::system_file(&project);
    let (app, cx) = f.open(cx, Some(project.clone()));
    let read = || std::fs::read_to_string(&file).expect("read");
    // Opening, selecting and an edit that changes nothing write nothing.
    app.update(cx, |app, cx| {
        app.select(Some("api".into()), cx);
        app.edit(cx, |_| {});
    });
    draw(cx);
    assert_eq!(read(), text);
    assert!(!project.join(".architecture/README.md").exists());

    // Rename (typed: one undo step), connect, then delete.
    app.update(cx, |app, cx| {
        for name in ["A", "Ap", "Api gateway"] {
            app.edit_grouped(Some("api:name"), cx, |s| s.nodes[0].name = name.into());
        }
        app.edit(cx, |s| {
            s.connect(
                "db",
                "api",
                (None, None),
                ferrumsystem_core::EdgeStyle::Async,
            )
        });
        app.select(Some("db".into()), cx);
        app.delete_selected(cx);
    });
    let saved = read();
    assert!(saved.contains(r#""x-owner": "platform""#), "{saved}");
    assert!(
        saved.contains(r#"{ "id": "api", "name": "Api gateway", "kind": "service", "layer": "services", "x-sla": "99.9" }"#),
        "{saved}"
    );
    assert!(!saved.contains(r#""id": "db""#), "{saved}");

    // Undo the delete, the connection and the rename; the file follows.
    app.update(cx, |app, cx| app.undo(false, cx));
    assert!(read().contains(r#"{ "id": "db", "name": "DB", "kind": "sql-db", "layer": "data" }"#));
    app.update(cx, |app, cx| {
        app.undo(false, cx);
        app.undo(false, cx);
        assert!(!app.history.can_undo());
        assert_eq!(app.system.nodes[0].name, "API");
    });
    assert_eq!(read(), text, "undone back to the very same text");
    cx.simulate_keystrokes("cmd-shift-z");
    app.read_with(cx, |app, _| {
        assert_eq!(app.system.nodes[0].name, "Api gateway")
    });
    cx.simulate_keystrokes("cmd-z");
    app.read_with(cx, |app, _| assert_eq!(app.system.nodes[0].name, "API"));
}

#[gpui::test]
fn drag_and_drop_moves_a_component_and_undo_puts_it_back(cx: &mut TestAppContext) {
    let f = Fixture::new();
    f.set_prefs(&seen());
    let (project, _) = hand_written(&f);
    let (app, cx) = f.open(cx, Some(project.clone()));
    app.update(cx, |app, cx| {
        app.drop_node("db", "services", cx);
        assert_eq!(
            app.system.node("db").map(|n| n.layer.as_str()),
            Some("services")
        );
        app.drop_handle("db", ferrumsystem_core::Side::Right, "api", cx);
        assert!(
            app.system
                .edges
                .iter()
                .any(|e| e.from == "db" && e.to == "api")
        );
        app.undo(false, cx);
        app.undo(false, cx);
        assert_eq!(
            app.system.node("db").map(|n| n.layer.as_str()),
            Some("data")
        );
    });
    let reloaded = store::load(&project).expect("load").expect("system");
    assert_eq!(reloaded.node("db").map(|n| n.layer.as_str()), Some("data"));
}

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

    fn set_prefs(&self, prefs: &UiPrefs) {
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

fn seen() -> UiPrefs {
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

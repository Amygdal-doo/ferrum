//! Headless GPUI tests of the whole window: the three real views (chat on
//! an isolated config, git on temporary services, terminals on a private
//! in-process server), drawn and driven without showing a window.

use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use ferrumagent_ui::core::session::SessionConfig;
use ferrumagent_ui::{ChatEvent, ChatOptions};
use ferrumgit_ui::GitViewEvent;
use gpui::{Entity, Modifiers, MouseButton, TestAppContext, VisualTestContext, point, px, size};
use tempfile::TempDir;

use crate::app::{FerrumAgentic, Options};
use crate::layout::{self, Column, Layout, Side};

type Opened = Rc<RefCell<Vec<(Option<PathBuf>, PathBuf, Option<u32>)>>>;

struct Fixture {
    dir: TempDir,
    opened: Opened,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        for sub in ["a", "b", "home", "git-data"] {
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

    fn options(&self, project: Option<PathBuf>) -> Options {
        let home = self.path("home");
        let opened = self.opened.clone();
        Options {
            project,
            layouts_file: Some(self.layouts_file()),
            projects_file: Some(self.dir.path().join("projects.json")),
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
    ) -> (Entity<FerrumAgentic>, &'a mut VisualTestContext) {
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
        let (app, cx) = cx.add_window_view(|window, cx| FerrumAgentic::new(options, window, cx));
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
    app: &Entity<FerrumAgentic>,
    cx: &mut VisualTestContext,
    f: impl FnOnce(&FerrumAgentic) -> R,
) -> R {
    app.read_with(cx, |app, _| f(app))
}

/// Columns drawn on screen, left to right.
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

#[gpui::test]
fn renders_three_columns_and_wires_their_events(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let root = fx.path("a");
    let (app, cx) = fx.open(cx, Some(root.clone()));
    assert_eq!(drawn(cx), Column::ALL);
    let (chat, git) = read(&app, cx, |a| {
        assert!(a.term.is_some());
        (a.chat.clone().expect("chat"), a.git.clone().expect("git"))
    });
    // Narrow windows still lay out (each column has a minimum when dragged,
    // not when the window shrinks).
    cx.simulate_resize(size(px(760.), px(520.)));
    draw(cx);
    assert_eq!(drawn(cx), Column::ALL);

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

    // Agent changes reload the git column and show in the chat header.
    let before = read(&app, cx, |a| a.git_refreshes);
    chat.update(cx, |_, cx| cx.emit(ChatEvent::ChangesUpdated { files: 2 }));
    draw(cx);
    assert_eq!(
        read(&app, cx, |a| (a.pending_changes, a.git_refreshes)),
        (2, before + 1)
    );

    // "Run in terminal" reopens a closed terminal column.
    cx.update(|window, cx| {
        app.update(cx, |a, cx| a.set_visible(Column::Term, false, window, cx));
    });
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Git]);
    chat.update(cx, |_, cx| {
        cx.emit(ChatEvent::RunInTerminal {
            cmd: "echo hi".into(),
            cwd: root.clone(),
        });
    });
    draw(cx);
    assert_eq!(drawn(cx), Column::ALL);
}

#[gpui::test]
fn columns_close_reopen_and_reorder(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, Some(fx.path("a")));

    // ⌥⌘2 hides and shows the git column.
    cx.simulate_keystrokes("cmd-alt-2");
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Term]);
    cx.simulate_keystrokes("cmd-alt-2");
    draw(cx);
    assert_eq!(drawn(cx), Column::ALL);

    // Every column can be closed; the window then says how to reopen one.
    cx.update(|window, cx| {
        app.update(cx, |a, cx| {
            for column in Column::ALL {
                a.set_visible(column, false, window, cx);
            }
        });
    });
    draw(cx);
    assert!(drawn(cx).is_empty());
    cx.update(|window, cx| {
        app.update(cx, |a, cx| {
            for column in Column::ALL {
                a.toggle(column, window, cx);
            }
        });
    });
    draw(cx);
    assert_eq!(drawn(cx), Column::ALL);

    // ⋯ → Move right / Move left, and dropping a header on another column.
    app.update(cx, |a, cx| a.move_column(Column::Chat, Side::Right, cx));
    draw(cx);
    assert_eq!(drawn(cx), [Column::Git, Column::Chat, Column::Term]);
    app.update(cx, |a, cx| a.move_column(Column::Term, Side::Left, cx));
    draw(cx);
    assert_eq!(drawn(cx), [Column::Git, Column::Term, Column::Chat]);
    app.update(cx, |a, cx| a.drop_column(Column::Chat, Column::Git, cx));
    draw(cx);
    assert_eq!(drawn(cx), [Column::Chat, Column::Git, Column::Term]);
}

#[gpui::test]
fn dragging_a_splitter_resizes_its_neighbours(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (_app, cx) = fx.open(cx, Some(fx.path("a")));
    let (chat, git, term) = (
        width(cx, "column-chat"),
        width(cx, "column-git"),
        width(cx, "column-term"),
    );
    let splitter = cx.debug_bounds("splitter-chat").expect("splitter");
    let start = splitter.center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    for step in 1..=4 {
        let at = point(start.x + px(40. * step as f32), start.y);
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    let end = point(start.x + px(160.), start.y);
    cx.simulate_mouse_up(end, MouseButton::Left, Modifiers::none());
    draw(cx);
    let (chat2, git2, term2) = (
        width(cx, "column-chat"),
        width(cx, "column-git"),
        width(cx, "column-term"),
    );
    assert!((chat2 - chat - 160.).abs() < 6., "{chat} → {chat2}");
    assert!((git - git2 - 160.).abs() < 6., "{git} → {git2}");
    assert!((term2 - term).abs() < 2., "{term} → {term2}");

    // Git never gets narrower than the minimum.
    let start = cx.debug_bounds("splitter-chat").expect("splitter").center();
    cx.simulate_mouse_down(start, MouseButton::Left, Modifiers::none());
    for step in 1..=10 {
        let at = point(start.x + px(100. * step as f32), start.y);
        cx.simulate_mouse_move(at, MouseButton::Left, Modifiers::none());
    }
    cx.simulate_mouse_up(start, MouseButton::Left, Modifiers::none());
    draw(cx);
    assert!(width(cx, "column-git") >= crate::app::MIN_COLUMN_WIDTH - 6.);
}

#[gpui::test]
fn layouts_are_kept_per_project(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (a, b) = (fx.path("a"), fx.path("b"));
    let (app, cx) = fx.open(cx, Some(a.clone()));

    // Project a: git closed, terminal first.
    cx.update(|window, cx| {
        app.update(cx, |app, cx| {
            app.set_visible(Column::Git, false, window, cx);
            app.drop_column(Column::Term, Column::Chat, cx);
        });
    });
    let layout_a = read(&app, cx, |app| app.layout.clone());
    assert_eq!(layout_a.visible(), [Column::Term, Column::Chat]);
    assert_eq!(layout::load(&fx.layouts_file(), Some(&a)), layout_a);

    // Project b starts from the last layout; change it to all three.
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.set_project(b.clone(), window, cx));
    });
    draw(cx);
    assert_eq!(read(&app, cx, |app| app.project.clone()), Some(b.clone()));
    assert_eq!(drawn(cx), [Column::Term, Column::Chat]);
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.set_visible(Column::Git, true, window, cx));
    });
    draw(cx);
    let layout_b = read(&app, cx, |app| app.layout.clone());
    assert_eq!(
        layout_b.visible(),
        [Column::Term, Column::Chat, Column::Git]
    );

    // Back to a: its own layout returns; the chat is a new one for a.
    let chat_b = read(&app, cx, |app| app.chat.clone().expect("chat"));
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.set_project(a.clone(), window, cx));
    });
    draw(cx);
    assert_eq!(drawn(cx), [Column::Term, Column::Chat]);
    let chat_a = read(&app, cx, |app| app.chat.clone().expect("chat"));
    assert_ne!(chat_a.entity_id(), chat_b.entity_id());

    // Round trip through the file.
    assert_eq!(layout::load(&fx.layouts_file(), Some(&a)), layout_a);
    assert_eq!(layout::load(&fx.layouts_file(), Some(&b)), layout_b);
    assert_ne!(layout_b, Layout::default());
}

#[gpui::test]
fn without_a_project_only_the_terminal_starts(cx: &mut TestAppContext) {
    let fx = Fixture::new();
    let (app, cx) = fx.open(cx, None);
    assert_eq!(drawn(cx), Column::ALL);
    read(&app, cx, |app| {
        assert!(app.chat.is_none() && app.git.is_none() && app.term.is_some());
    });
    cx.update(|window, cx| {
        app.update(cx, |app, cx| app.set_project(fx.path("a"), window, cx));
    });
    draw(cx);
    read(&app, cx, |app| {
        assert!(app.chat.is_some() && app.git.is_some())
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

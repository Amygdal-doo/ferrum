//! The tool panes, headless: Git, Agent and Term open from the top bar at
//! the same time, move between docks, resize, close, come back from the
//! session (per project), and their events reach the editor. The terminal
//! runs on a private server and commands are recorded, not run; the chat
//! uses an isolated configuration; FerrumGIT's services use a temp folder.

use std::path::{Path, PathBuf};

use ferrumagent_ui::ChatEvent;
use ferrumeditor_core::tool_layout::{Dock, ToolKind};
use ferrumgit_ui::GitViewEvent;
use gpui::{Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::tests::{init_with_user_dir, primary};
use crate::workspace::Workspace;

/// FerrumGIT's background services on a temp folder (never the user's).
fn git_services(dir: &Path, cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let services = ferrumgit_ui::AppServices::start(dir).expect("git services");
    cx.update(|cx| ferrumgit_ui::init_with_services(services, cx));
}

struct Env {
    _home: tempfile::TempDir,
    _git_data: tempfile::TempDir,
    project: tempfile::TempDir,
}

impl Env {
    fn root(&self) -> PathBuf {
        self.project.path().canonicalize().expect("canonical")
    }
}

fn setup(cx: &mut TestAppContext) -> Env {
    let env = Env {
        _home: tempfile::tempdir().expect("home"),
        _git_data: tempfile::tempdir().expect("git data"),
        project: tempfile::tempdir().expect("project"),
    };
    std::fs::write(env.project.path().join("a.txt"), "one\ntwo\nthree\n").expect("write");
    init_with_user_dir(cx, env._home.path());
    git_services(env._git_data.path(), cx);
    env
}

fn open_workspace<'a>(
    root: &Path,
    cx: &'a mut TestAppContext,
) -> (Entity<Workspace>, &'a mut VisualTestContext) {
    let (workspace, cx) = cx.add_window_view(Workspace::new);
    let root = root.to_path_buf();
    workspace.update_in(cx, |w, window, cx| {
        w.tools_mut().real_term = true;
        w.set_root(root, window, cx)
    });
    cx.run_until_parked();
    (workspace, cx)
}

fn click_top_bar(selector: &'static str, cx: &mut VisualTestContext) {
    let bounds = cx
        .debug_bounds(selector)
        .unwrap_or_else(|| panic!("{selector} in the top bar"));
    cx.simulate_click(bounds.center(), Modifiers::none());
    cx.run_until_parked();
}

fn visible(workspace: &Entity<Workspace>, dock: Dock, cx: &mut VisualTestContext) -> Vec<ToolKind> {
    workspace.read_with(cx, |w, _| {
        w.tool_layout()
            .visible(dock)
            .into_iter()
            .map(|(k, _)| k)
            .collect()
    })
}

#[gpui::test]
fn git_agent_and_term_open_together_move_resize_close_and_restore(cx: &mut TestAppContext) {
    let env = setup(cx);
    let root = env.root();
    let (workspace, cx) = open_workspace(&root, cx);

    // The three top-bar buttons open their panes inside the window.
    for selector in ["top-bar-git", "top-bar-agent", "top-bar-terminal"] {
        click_top_bar(selector, cx);
    }
    workspace.read_with(cx, |w, _| {
        for kind in [ToolKind::Git, ToolKind::Agent, ToolKind::Term] {
            assert!(w.tool_open(kind), "{kind:?} open");
        }
        let tools = w.tools();
        assert!(tools.git.is_some() && tools.chat.is_some() && tools.term.is_some());
    });
    assert_eq!(
        visible(&workspace, Dock::Right, cx),
        [ToolKind::Git, ToolKind::Agent]
    );
    assert_eq!(visible(&workspace, Dock::Bottom, cx), [ToolKind::Term]);
    assert!(
        cx.debug_bounds("tool-pane-git").is_some(),
        "the Git pane is drawn"
    );
    assert!(
        cx.debug_bounds("tool-pane-terminal").is_some(),
        "the Term pane is drawn"
    );

    // Move the Agent pane to the bottom, next to the terminal; resize.
    workspace.update(cx, |w, cx| {
        w.move_tool(ToolKind::Agent, Dock::Bottom, cx);
        let layout = &mut w.tools_mut().layout;
        layout.set_dock_size(Dock::Right, 520.);
        layout.set_dock_size(Dock::Bottom, 300.);
        layout.drag_divider(Dock::Bottom, 0, 0.3);
    });
    cx.run_until_parked();
    assert_eq!(visible(&workspace, Dock::Right, cx), [ToolKind::Git]);
    assert_eq!(
        visible(&workspace, Dock::Bottom, cx),
        [ToolKind::Term, ToolKind::Agent]
    );
    assert!(cx.debug_bounds("tool-pane-agent").is_some());

    // Clicking Git again closes its pane; the others stay.
    click_top_bar("top-bar-git", cx);
    assert!(!workspace.read_with(cx, |w, _| w.tool_open(ToolKind::Git)));
    assert!(cx.debug_bounds("tool-pane-git").is_none());
    let saved = workspace.update(cx, |w, cx| {
        w.save_session(cx);
        w.tool_layout().clone()
    });

    // A new window restores the layout (and views) of the project.
    let (second, cx) = cx.add_window_view(Workspace::new);
    assert!(second.update_in(cx, |w, window, cx| {
        w.tools_mut().real_term = true;
        w.restore_session(window, cx)
    }));
    cx.run_until_parked();
    second.read_with(cx, |w, _| {
        assert_eq!(w.tool_layout(), &saved);
        assert_eq!(w.tool_layout().dock_size(Dock::Right), 520.);
        let shares = w.tool_layout().visible(Dock::Bottom);
        assert!((shares[0].1 - 0.3).abs() < 1e-4, "{shares:?}");
        assert!(w.tools().term.is_some() && w.tools().chat.is_some());
        assert!(w.tools().git.is_none(), "closed panes are not recreated");
    });
}

#[gpui::test]
fn layouts_are_per_project(cx: &mut TestAppContext) {
    let env = setup(cx);
    let a = env.root();
    let other = tempfile::tempdir().expect("b");
    let b = other.path().canonicalize().expect("canonical");
    let (workspace, cx) = open_workspace(&a, cx);
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |w, _| w.tool_open(ToolKind::Term)));
    workspace.update(cx, |w, cx| w.move_tool(ToolKind::Term, Dock::Left, cx));

    let set_root = |path: &PathBuf, cx: &mut VisualTestContext| {
        let path = path.clone();
        workspace.update_in(cx, |w, window, cx| w.set_root(path, window, cx));
        cx.run_until_parked();
    };
    // A new project starts from the current layout, then goes its own way.
    set_root(&b, cx);
    assert_eq!(visible(&workspace, Dock::Left, cx), [ToolKind::Term]);
    workspace.update_in(cx, |w, window, cx| {
        w.close_tool(ToolKind::Term, window, cx);
        w.open_tool(ToolKind::Git, window, cx);
    });
    set_root(&a, cx);
    assert_eq!(visible(&workspace, Dock::Left, cx), [ToolKind::Term]);
    assert!(visible(&workspace, Dock::Right, cx).is_empty());
    set_root(&b, cx);
    assert!(visible(&workspace, Dock::Left, cx).is_empty());
    assert_eq!(visible(&workspace, Dock::Right, cx), [ToolKind::Git]);
}

#[gpui::test]
fn shortcuts_toggle_the_embedded_panes(cx: &mut TestAppContext) {
    let env = setup(cx);
    let (workspace, cx) = open_workspace(&env.root(), cx);
    workspace.update_in(cx, |w, window, cx| w.new_file(window, cx));
    let open = |kind, cx: &mut VisualTestContext| workspace.read_with(cx, |w, _| w.tool_open(kind));

    cx.simulate_keystrokes(&primary("cmd-alt-g"));
    cx.run_until_parked();
    assert!(open(ToolKind::Git, cx));
    cx.simulate_keystrokes(&primary("cmd-alt-a"));
    cx.run_until_parked();
    assert!(
        open(ToolKind::Agent, cx) && open(ToolKind::Git, cx),
        "both stay open"
    );
    // ⌥⌘A again, with the chat focused, closes it.
    cx.simulate_keystrokes(&primary("cmd-alt-a"));
    cx.run_until_parked();
    assert!(!open(ToolKind::Agent, cx));
    // ⌃` opens the terminal; ⌃` from inside it closes it again.
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    assert!(open(ToolKind::Term, cx));
    assert!(cx.update(|window, cx| workspace.read(cx).tool_focused(ToolKind::Term, window, cx)));
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    assert!(!open(ToolKind::Term, cx));
}

#[gpui::test]
fn tool_events_open_files_run_commands_and_reload_buffers(cx: &mut TestAppContext) {
    let env = setup(cx);
    let root = env.root();
    let file = root.join("a.txt");
    let (workspace, cx) = open_workspace(&root, cx);
    for kind in [ToolKind::Git, ToolKind::Agent] {
        workspace.update_in(cx, |w, window, cx| w.open_tool(kind, window, cx));
    }
    cx.run_until_parked();
    let (git, chat) = workspace.read_with(cx, |w, _| {
        (
            w.tools().git.clone().expect("git view"),
            w.tools().chat.clone().expect("chat view"),
        )
    });
    let cursor = |cx: &mut VisualTestContext| {
        let editor = workspace
            .read_with(cx, |w, _| w.active_editor())
            .expect("an editor");
        editor.read_with(cx, |e, _| (e.path.clone(), e.cursor_point().row))
    };

    // Git's "Open" opens the file at the (1-based) line.
    git.update(cx, |_, cx| {
        cx.emit(GitViewEvent::OpenFile {
            path: file.clone(),
            line: Some(3),
        })
    });
    cx.run_until_parked();
    assert_eq!(cursor(cx), (Some(file.clone()), 2));

    // So does the chat's.
    chat.update(cx, |_, cx| {
        cx.emit(ChatEvent::OpenFile {
            path: file.clone(),
            line: Some(2),
        })
    });
    cx.run_until_parked();
    assert_eq!(cursor(cx), (Some(file.clone()), 1));

    // "Run in terminal" opens the Term pane and runs the command there.
    chat.update(cx, |_, cx| {
        cx.emit(ChatEvent::RunInTerminal {
            cmd: "cargo test".into(),
            cwd: root.clone(),
        })
    });
    cx.run_until_parked();
    workspace.read_with(cx, |w, _| {
        assert!(w.tool_open(ToolKind::Term));
        assert_eq!(
            w.tools().dry_run.as_deref(),
            Some(&["cargo test".to_string()][..])
        );
    });

    // The agent changed the file on disk: the clean buffer reloads.
    std::fs::write(&file, "ONE\n").expect("write");
    chat.update(cx, |_, cx| cx.emit(ChatEvent::ChangesUpdated { files: 1 }));
    cx.run_until_parked();
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");
    assert_eq!(editor.read_with(cx, |e, _| e.state.text()), "ONE\n");
}

#[gpui::test]
fn a_notification_from_another_projects_chat_switches_to_that_project(cx: &mut TestAppContext) {
    let env = setup(cx);
    let a = env.root();
    let other = tempfile::tempdir().expect("b");
    let b = other.path().canonicalize().expect("canonical");
    let (workspace, cx) = open_workspace(&a, cx);
    workspace.update_in(cx, |w, window, cx| w.open_tool(ToolKind::Agent, window, cx));
    cx.run_until_parked();
    let a_chat = workspace.read_with(cx, |w, _| w.tools().chat.clone().expect("chat"));
    let to_b = b.clone();
    workspace.update_in(cx, |w, window, cx| {
        w.set_root(to_b, window, cx);
        w.close_tool(ToolKind::Agent, window, cx);
    });
    cx.run_until_parked();
    workspace.read_with(cx, |w, _| {
        assert_ne!(w.tools().chat.as_ref(), Some(&a_chat), "B has its own chat");
    });

    // A's chat (hidden, kept) reports a clicked notification.
    a_chat.update(cx, |_, cx| {
        cx.emit(ChatEvent::Reveal {
            thread_id: "t".into(),
        })
    });
    cx.run_until_parked();
    workspace.read_with(cx, |w, _| {
        assert_eq!(w.root.as_deref(), Some(a.as_path()));
        assert_eq!(w.tools().chat.as_ref(), Some(&a_chat));
        assert!(w.tool_open(ToolKind::Agent));
    });
}

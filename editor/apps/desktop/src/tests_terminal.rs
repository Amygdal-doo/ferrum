//! Headless tests of the terminal panel: toggling by command and from the
//! status bar, keyboard input reaching the PTY (including keys bound
//! window-wide elsewhere), copy / paste, tabs. Sessions are detached (no
//! real shell): what the terminal sends is recorded per session.

use gpui::{ClipboardItem, Entity, Modifiers, TestAppContext, VisualTestContext};

use crate::settings::MAC;
use crate::terminal::{TerminalView, test_support};
use crate::tests::workspace_window;
use crate::workspace::Workspace;

fn panel_state(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> (bool, usize) {
    let panel = workspace.read_with(cx, |w, _| w.terminal().clone());
    panel.read_with(cx, |p, _| (p.is_open(), p.len()))
}

fn terminal_focused(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> bool {
    let panel = workspace.read_with(cx, |w, _| w.terminal().clone());
    cx.update(|window, cx| panel.read(cx).is_focused(window, cx))
}

fn active_view(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<TerminalView> {
    let panel = workspace.read_with(cx, |w, _| w.terminal().clone());
    panel
        .read_with(cx, |p, _| p.active())
        .expect("an active terminal")
}

#[gpui::test]
fn terminal_toggles_by_command_and_takes_keyboard_input(cx: &mut TestAppContext) {
    test_support::reset();
    let (workspace, cx) = workspace_window(cx);
    workspace.update_in(cx, |w, window, cx| w.new_file(window, cx));
    cx.run_until_parked();
    assert_eq!(panel_state(&workspace, cx), (false, 0));

    // ⌃` opens the panel with one shell and focuses it.
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    assert_eq!(panel_state(&workspace, cx), (true, 1));
    assert!(terminal_focused(&workspace, cx));

    // Keys go to the PTY: Ctrl+C, text, Enter, arrows — and keys that are
    // window-wide bindings elsewhere (Ctrl+G = go to line, Escape).
    cx.simulate_keystrokes("ctrl-c");
    cx.simulate_input("ls");
    cx.simulate_keystrokes("enter up ctrl-g escape tab alt-f");
    assert_eq!(
        test_support::sent(0),
        b"\x03ls\r\x1b[A\x07\x1b\t\x1bf".to_vec()
    );
    assert!(
        !workspace.read_with(cx, |w, _| w.picker_open()),
        "Ctrl+G did not open go-to-line"
    );

    // Paste goes to the shell; copy takes the mouse selection.
    cx.write_to_clipboard(ClipboardItem::new_string("echo hi".into()));
    cx.simulate_keystrokes(if MAC { "cmd-v" } else { "ctrl-shift-v" });
    assert!(test_support::sent(0).ends_with(b"echo hi"));
    let view = active_view(&workspace, cx);
    view.update(cx, |v, _| {
        v.session().feed(b"hello world");
        v.select_cells((0, 0), (0, 4));
    });
    cx.simulate_keystrokes(if MAC { "cmd-c" } else { "ctrl-shift-c" });
    assert_eq!(
        cx.read_from_clipboard().and_then(|c| c.text()).as_deref(),
        Some("hello")
    );

    // ⌃` while focused hides the panel and returns to the editor; the
    // shell keeps running.
    cx.simulate_keystrokes("ctrl-`");
    cx.run_until_parked();
    assert_eq!(panel_state(&workspace, cx), (false, 1));
    assert!(!terminal_focused(&workspace, cx));
    cx.simulate_input("x");
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");
    assert_eq!(editor.read_with(cx, |e, _| e.state.text()), "x");

    // Reopening shows the same terminal.
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("terminal:toggle", window, cx)
    });
    cx.run_until_parked();
    assert_eq!(panel_state(&workspace, cx), (true, 1));
    assert!(terminal_focused(&workspace, cx));
}

#[gpui::test]
fn status_bar_button_toggles_the_panel(cx: &mut TestAppContext) {
    test_support::reset();
    let (workspace, cx) = workspace_window(cx);
    cx.run_until_parked();
    let button = cx
        .debug_bounds("status-terminal-toggle")
        .expect("terminal button in the status bar");
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(panel_state(&workspace, cx), (true, 1));
    let button = cx
        .debug_bounds("status-terminal-toggle")
        .expect("terminal button in the status bar");
    cx.simulate_click(button.center(), Modifiers::none());
    cx.run_until_parked();
    assert_eq!(panel_state(&workspace, cx), (false, 1));
}

#[gpui::test]
fn terminal_tabs_new_cycle_and_close(cx: &mut TestAppContext) {
    test_support::reset();
    let (workspace, cx) = workspace_window(cx);
    let run = |name: &'static str, cx: &mut VisualTestContext| {
        workspace.update_in(cx, |w, window, cx| w.run_command(name, window, cx));
        cx.run_until_parked();
    };
    run("terminal:new", cx);
    run("terminal:new", cx);
    assert_eq!(panel_state(&workspace, cx), (true, 2));
    let second = active_view(&workspace, cx).read_with(cx, |v, _| v.id);
    cx.simulate_keystrokes(if MAC { "cmd-shift-]" } else { "ctrl-shift-]" });
    let first = active_view(&workspace, cx).read_with(cx, |v, _| v.id);
    assert_ne!(first, second, "next tab cycles");
    cx.simulate_input("a");
    assert_eq!(
        test_support::sent(0),
        b"a".to_vec(),
        "typing goes to the active tab"
    );
    run("terminal:close", cx);
    assert_eq!(panel_state(&workspace, cx), (true, 1));
    run("terminal:close", cx);
    assert_eq!(panel_state(&workspace, cx), (false, 0));
}

#[test]
fn terminal_owns_shell_keys_but_not_app_shortcuts() {
    use crate::actions::terminal_owns;
    assert!(terminal_owns("ctrl-g"));
    assert!(terminal_owns("alt-z"));
    assert!(terminal_owns("ctrl-tab"));
    if MAC {
        assert!(!terminal_owns("cmd-p"));
        assert!(!terminal_owns("cmd-k right"));
    } else {
        assert!(terminal_owns("ctrl-p"));
        assert!(!terminal_owns("ctrl-shift-p"));
    }
}

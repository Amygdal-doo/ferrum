//! Workspace side of the integrated terminal: the `terminal:*` commands,
//! focus moves between editor and terminal, the status bar toggle and
//! "Open in FerrumTerm".

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ui::{IconButton, IconName, IconSize};

use super::Workspace;
use crate::terminal::PanelEvent;

impl Workspace {
    /// `terminal:toggle` (VS Code style): closed → open and focus; open but
    /// not focused → focus; focused → hide and return to the editor.
    pub(super) fn toggle_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panel = self.terminal.clone();
        let (open, focused) = {
            let p = panel.read(cx);
            (p.is_open(), p.is_focused(window, cx))
        };
        match (open, focused) {
            (true, true) => self.hide_terminal(window, cx),
            (true, false) => panel.update(cx, |p, cx| p.focus_active(window, cx)),
            (false, _) => self.show_terminal(window, cx),
        }
        cx.notify();
    }

    /// The status bar button: a plain open / hide switch.
    pub(super) fn toggle_terminal_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal.read(cx).is_open() {
            self.hide_terminal(window, cx);
        } else {
            self.show_terminal(window, cx);
        }
        cx.notify();
    }

    fn show_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let result = self.terminal.update(cx, |p, cx| {
            p.cwd = root;
            p.show(window, cx)?;
            p.focus_active(window, cx);
            Ok::<(), String>(())
        });
        if let Err(error) = result {
            self.error(format!("Could not start a terminal: {error}"), cx);
        }
    }

    fn hide_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal.update(cx, |p, cx| p.hide(cx));
        self.focus_active(window, cx);
    }

    pub(super) fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let root = self.root.clone();
        let result = self.terminal.update(cx, |p, cx| {
            p.cwd = root;
            p.new_terminal(window, cx)
        });
        if let Err(error) = result {
            self.error(format!("Could not start a terminal: {error}"), cx);
        }
        cx.notify();
    }

    pub(super) fn close_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal.update(cx, |p, cx| p.close_active(window, cx));
        cx.notify();
    }

    pub(super) fn cycle_terminal(
        &mut self,
        delta: isize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal.update(cx, |p, cx| p.cycle(delta, window, cx));
    }

    pub(super) fn on_terminal_event(
        &mut self,
        _: &Entity<crate::terminal::TerminalPanel>,
        event: &PanelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            PanelEvent::Hidden => self.focus_active(window, cx),
            PanelEvent::TaskFinished { .. } => self.on_task_finished(event, cx),
            PanelEvent::Focused => {}
        }
        cx.notify();
    }

    /// Launches FerrumTerm in the project folder, if installed.
    pub(super) fn open_in_ferrumterm(&mut self, cx: &mut Context<Self>) {
        let dir = self
            .root
            .clone()
            .or_else(|| std::env::current_dir().ok())
            .unwrap_or_default();
        let name = dir
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "editor".into());
        let fterm = find_on_path("fterm");
        let gui = find_on_path("ferrumterm");
        if fterm.is_none() && gui.is_none() {
            self.info(
                "FerrumTerm is not installed (no `ferrumterm` or `fterm` on PATH)",
                cx,
            );
            return;
        }
        if let Some(fterm) = fterm {
            // A session in the folder; the window (if any) shows it.
            let spawned = std::process::Command::new(fterm)
                .args(["new", &name, "-c"])
                .arg(&dir)
                .current_dir(&dir)
                .spawn();
            if let Err(error) = spawned {
                self.error(format!("Could not run fterm: {error}"), cx);
                return;
            }
        }
        if let Some(gui) = gui
            && let Err(error) = std::process::Command::new(gui).current_dir(&dir).spawn()
        {
            self.error(format!("Could not open FerrumTerm: {error}"), cx);
            return;
        }
        self.info(format!("Opened {} in FerrumTerm", dir.display()), cx);
    }

    /// Bottom-right status bar button; highlighted while the panel is open.
    pub(super) fn render_terminal_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.terminal.read(cx).is_open();
        div()
            .debug_selector(|| "status-terminal-toggle".into())
            .child(
                IconButton::new("status-terminal-toggle", IconName::Terminal)
                    .icon_size(IconSize::Small)
                    .toggle_state(open)
                    .when(open, |b| b.icon_color(Color::Accent))
                    .tooltip(Tooltip::text(if open {
                        "Hide Terminal"
                    } else {
                        "Show Terminal"
                    }))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.toggle_terminal_panel(window, cx)),
                    ),
            )
    }
}

fn find_on_path(program: &str) -> Option<PathBuf> {
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|candidate| is_executable(candidate))
}

fn is_executable(path: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt as _;
        path.metadata()
            .is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
    }
    #[cfg(not(unix))]
    {
        path.is_file()
    }
}

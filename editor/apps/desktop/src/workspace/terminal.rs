//! The `terminal:*` commands for the Term pane (FerrumTerm's view), the
//! status bar toggle, and "Open in FerrumTerm app".

use std::path::{Path, PathBuf};

use ferrum_ui::components::{IconButton, IconName, IconSize};
use ferrum_ui::prelude::*;

use ferrumeditor_core::tool_layout::ToolKind;

use super::Workspace;
use crate::registry::CommandRegistry;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("terminal:toggle", |w, window, cx| {
        w.toggle_tool_focus(ToolKind::Term, window, cx)
    });
    r.register("terminal:new", |w, window, cx| w.new_terminal(window, cx));
    r.register("terminal:open-in-ferrumterm", |w, _, cx| {
        w.open_in_ferrumterm(cx)
    });
}

impl Workspace {
    /// A new shell tab in the Term pane (opened and focused).
    fn new_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let existed = self.tools.term.is_some();
        self.open_tool(ToolKind::Term, window, cx);
        if existed && let Some(term) = self.tools.term.clone() {
            let cwd = self.root.clone();
            term.update(cx, |t, cx| t.new_tab(cwd, cx));
        }
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

    /// Bottom-right status bar button; highlighted while the pane is open.
    pub(super) fn render_terminal_button(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.tool_open(ToolKind::Term);
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
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_tool(ToolKind::Term, window, cx)
                    })),
            )
    }
}

pub(crate) fn find_on_path(program: &str) -> Option<PathBuf> {
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

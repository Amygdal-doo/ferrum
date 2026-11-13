//! The debugger in the workspace: the `debug:*` commands (F5, F9, F10,
//! F11…), the launch configuration picker, keeping every editor's
//! breakpoint and execution marks current, opening the place a session
//! stopped at, running `runInTerminal` programs in a terminal tab, and the
//! status bar item.

use std::path::Path;

use ferrum_ui::prelude::*;
use ferrumeditor_core::dap::{AdapterKind, LaunchConfig};

use super::Workspace;
use crate::debug::{DebugPanel, DebugPanelEvent, Debugger, DebuggerEvent, configurations};
use crate::editor::{DebugMarks, EditorView};
use crate::picker::{PickerItem, PickerKind};
use crate::registry::CommandRegistry;
use ferrumeditor_core::tool_layout::ToolKind;

pub(super) fn register(r: &mut CommandRegistry) {
    r.register("debug:start", |w, window, cx| {
        w.toggle_picker(PickerKind::DebugConfigs, window, cx)
    });
    r.register("debug:continue", |w, window, cx| {
        let running = w.debugger.read(cx).session.is_some();
        if running {
            w.debugger.update(cx, |d, cx| d.step("continue", cx));
        } else {
            w.toggle_picker(PickerKind::DebugConfigs, window, cx);
        }
    });
    for (name, command) in [
        ("debug:step-over", "next"),
        ("debug:step-into", "stepIn"),
        ("debug:step-out", "stepOut"),
        ("debug:pause", "pause"),
    ] {
        r.register(name, move |w, _, cx| {
            w.debugger.update(cx, |d, cx| d.step(command, cx))
        });
    }
    r.register("debug:stop", |w, _, cx| {
        w.debugger.update(cx, |d, cx| d.stop(cx))
    });
    r.register("debug:restart", |w, _, cx| {
        w.show_debug_pane(cx);
        w.debugger.update(cx, |d, cx| d.restart(cx));
    });
    r.register("debug:toggle-breakpoint", |w, _, cx| {
        if let Some(editor) = w.active_editor() {
            let line = editor.read(cx).cursor_point().row;
            w.toggle_breakpoint_at(&editor, line, cx);
        }
    });
    r.register("debug:remove-all-breakpoints", |w, _, cx| {
        w.debugger.update(cx, |d, cx| d.remove_all_breakpoints(cx))
    });
    r.register("debug:toggle-panel", |w, window, cx| {
        w.toggle_tool_focus(ToolKind::Debug, window, cx)
    });
    r.register("debug:add-watch", |w, window, cx| {
        w.show_debug_pane(cx);
        let input = w.debug_panel.read(cx).watch_input();
        input.update(cx, |i, cx| i.focus(window, cx));
    });
    r.register("debug:install-adapter", |w, _, cx| {
        let adapter = w
            .active_editor()
            .and_then(|e| e.read(cx).path.clone())
            .and_then(|p| p.extension().map(|e| e.to_string_lossy().to_lowercase()));
        let adapter = match adapter.as_deref() {
            Some("go") => Some(AdapterKind::Delve),
            Some("js" | "mjs" | "cjs" | "ts" | "mts" | "tsx" | "jsx") => Some(AdapterKind::JsDebug),
            _ if w
                .root
                .as_deref()
                .is_some_and(|r| r.join("go.mod").is_file()) =>
            {
                Some(AdapterKind::Delve)
            }
            _ if w
                .root
                .as_deref()
                .is_some_and(|r| r.join("package.json").is_file()) =>
            {
                Some(AdapterKind::JsDebug)
            }
            _ => None,
        };
        match adapter {
            Some(adapter) => w
                .debugger
                .update(cx, |d, cx| d.install_adapter(adapter, cx)),
            None => w.info(
                "Open a Go or JS/TS file: Rust uses lldb-dap from Xcode's Command Line Tools",
                cx,
            ),
        }
    });
}

impl Workspace {
    fn launch_configs(&self, cx: &App) -> Option<(Vec<LaunchConfig>, Option<String>)> {
        let root = self.root.as_deref()?;
        let file = self.active_editor().and_then(|e| e.read(cx).path.clone());
        Some(configurations(root, file.as_deref()))
    }

    pub(super) fn debug_items(&mut self, cx: &mut Context<Self>) -> Option<Vec<PickerItem>> {
        let Some((configs, error)) = self.launch_configs(cx) else {
            self.info("Open a folder to debug it (⌘⇧O)", cx);
            return None;
        };
        if let Some(error) = error {
            self.error(error, cx);
        }
        if configs.is_empty() {
            self.info(
                "Nothing to debug: add .vscode/launch.json, or open a Rust, Go or JS/TS project",
                cx,
            );
            return None;
        }
        Some(
            configs
                .into_iter()
                .enumerate()
                .map(|(i, c)| {
                    PickerItem::new(c.name, format!("{} · {}", c.kind, c.source), i.to_string())
                })
                .collect(),
        )
    }

    pub(super) fn debug_payload(&mut self, payload: &str, cx: &mut Context<Self>) {
        let config = payload
            .parse::<usize>()
            .ok()
            .and_then(|i| self.launch_configs(cx)?.0.into_iter().nth(i));
        if let Some(config) = config {
            self.start_debugging(config, cx);
        }
    }

    /// Opens the Debug pane (it always exists) without moving focus.
    fn show_debug_pane(&mut self, cx: &mut Context<Self>) {
        self.tools.layout.set_open(ToolKind::Debug, true);
        cx.notify();
    }

    pub fn start_debugging(&mut self, config: LaunchConfig, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.show_debug_pane(cx);
        self.debugger.update(cx, |d, cx| d.start(config, root, cx));
    }

    /// Gutter click or F9 at 0-based `line`.
    pub(super) fn toggle_breakpoint_at(
        &mut self,
        editor: &Entity<EditorView>,
        line: usize,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = editor.read(cx).path.clone() else {
            self.info("Save the file to set breakpoints in it", cx);
            return;
        };
        self.debugger
            .update(cx, |d, cx| d.toggle_breakpoint(&path, line + 1, cx));
        self.sync_debug_marks(cx);
    }

    pub(super) fn sync_debug_marks_for(&self, editor: &Entity<EditorView>, cx: &mut Context<Self>) {
        let Some(path) = editor.read(cx).path.clone() else {
            return;
        };
        let (breakpoints, exec_line) = self.debugger.read(cx).marks_for(&path);
        editor.update(cx, |e, cx| {
            e.set_debug_marks(
                DebugMarks {
                    breakpoints,
                    exec_line,
                },
                cx,
            )
        });
    }

    fn sync_debug_marks(&self, cx: &mut Context<Self>) {
        for editor in self.all_editors() {
            self.sync_debug_marks_for(&editor, cx);
        }
    }

    pub(super) fn on_debugger_event(
        &mut self,
        _: &Entity<Debugger>,
        event: &DebuggerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            DebuggerEvent::Changed => {}
            DebuggerEvent::Stopped { path, line } => {
                if path.is_file() {
                    self.open_file_at(path, Some(line.saturating_sub(1)), None, window, cx);
                }
            }
            DebuggerEvent::RunInTerminal {
                title,
                command,
                cwd,
            } => {
                let cwd = if cwd.as_os_str().is_empty() {
                    self.root.clone().unwrap_or_default()
                } else {
                    cwd.clone()
                };
                self.run_task_in_terminal(title, command, Path::new(&cwd), window, cx);
            }
            DebuggerEvent::Notice(message) => self.info(message.clone(), cx),
            DebuggerEvent::Error(message) => self.error(message.clone(), cx),
        }
        self.sync_debug_marks(cx);
        cx.notify();
    }

    pub(super) fn on_debug_panel_event(
        &mut self,
        _: &Entity<DebugPanel>,
        event: &DebugPanelEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let DebugPanelEvent::Open { path, line } = event;
        self.open_file_at(path, Some(line.saturating_sub(1)), None, window, cx);
    }

    pub(super) fn render_debug_status(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let label = self.debugger.read(cx).status_label()?;
        Some(
            div()
                .id("status-debug")
                .cursor_pointer()
                .on_click(
                    cx.listener(|this, _, window, cx| {
                        this.toggle_tool(ToolKind::Debug, window, cx)
                    }),
                )
                .child(
                    Label::new(format!("⬢ {label}"))
                        .size(LabelSize::Small)
                        .color(Color::Warning),
                )
                .into_any_element(),
        )
    }
}

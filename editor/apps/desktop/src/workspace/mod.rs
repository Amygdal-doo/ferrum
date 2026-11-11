//! The window's root view: tree view, split panes of tabs, find bar,
//! find-in-files panel, the terminal panel, overlays (palette, file
//! finder, go to line, theme selectors), dialogs and the status bar.
//! Behaviour is split by topic into the submodules.

mod commands;
mod dialogs;
mod files;
mod lsp;
mod overlays;
mod panes;
mod session;
mod status_bar;
mod tasks;
mod terminal;
mod themes;
mod watcher;

use std::path::PathBuf;

use ferrum_ui::prelude::*;
use ferrum_ui::{Notice, render_notice};
use gpui::{Axis, DragMoveEvent};

pub use panes::{Pane, PaneItem, PaneNode};

use crate::actions::RunCommand;
use crate::editor::EditorView;
use crate::find_bar::FindBar;
use crate::picker::{Picker, PickerKind};
use crate::project_find::ProjectFind;
use crate::registry::CommandRegistry;
use crate::terminal::TerminalPanel;
use crate::tree_view::TreeView;

pub use dialogs::Dialog;

pub struct Workspace {
    focus_handle: FocusHandle,
    pub root: Option<PathBuf>,
    tree: Option<Entity<TreeView>>,
    tree_visible: bool,
    tree_width: Pixels,
    panes: Vec<Pane>,
    layout: PaneNode,
    active_pane: usize,
    next_pane_id: usize,
    closed_tabs: Vec<PathBuf>,
    registry: CommandRegistry,
    picker: Option<Entity<Picker>>,
    find_bar: Entity<FindBar>,
    find_visible: bool,
    project_find: Entity<ProjectFind>,
    project_find_visible: bool,
    project_find_height: Pixels,
    terminal: Entity<TerminalPanel>,
    /// Theme / icon theme in use when a selector opened (restored on Escape).
    theme_preview: Option<(PickerKind, String)>,
    dialog: Option<Dialog>,
    notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    branch: Option<String>,
    watcher: Option<watcher::Watcher>,
    file_list: Option<std::sync::Arc<Vec<PathBuf>>>,
    file_list_task: Option<Task<()>>,
    window_title: String,
    /// Places to return to after go to definition.
    nav_back: Vec<lsp::NavEntry>,
    nav_forward: Vec<lsp::NavEntry>,
    /// Items for the next Locations picker.
    location_items: Vec<crate::picker::PickerItem>,
    /// For tasks:rerun-last.
    last_task: Option<ferrumeditor_core::tasks::Task>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find_bar = cx.new(|cx| FindBar::new(window, cx));
        let project_find = cx.new(|cx| ProjectFind::new(window, cx));
        let terminal = cx.new(|_| TerminalPanel::new());
        let mut subscriptions = vec![
            cx.subscribe_in(&find_bar, window, Self::on_find_bar_event),
            cx.subscribe_in(&project_find, window, Self::on_project_find_event),
            cx.subscribe_in(&terminal, window, Self::on_terminal_event),
            // Files may change while the window is in the background.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.check_disk_changes(window, cx);
                }
            }),
        ];
        // Problem counts and server state live in the status bar.
        if let Some(store) = crate::lsp_store::LspStore::global(cx) {
            subscriptions.push(cx.observe(&store, |_, _, cx| cx.notify()));
            subscriptions.push(cx.subscribe(&store, |_, _, _, cx| cx.notify()));
        }
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            root: None,
            tree: None,
            tree_visible: true,
            tree_width: px(240.),
            panes: vec![Pane::new(0)],
            layout: PaneNode::Leaf(0),
            active_pane: 0,
            next_pane_id: 1,
            closed_tabs: Vec::new(),
            registry: commands::builtin_registry(),
            picker: None,
            find_bar,
            find_visible: false,
            project_find,
            project_find_visible: false,
            project_find_height: px(260.),
            terminal,
            theme_preview: None,
            dialog: None,
            notice: None,
            notice_task: None,
            branch: None,
            watcher: None,
            file_list: None,
            file_list_task: None,
            window_title: String::new(),
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            location_items: Vec::new(),
            last_task: None,
            _subscriptions: subscriptions,
        };
        this.start_watcher(window, cx);
        // Keep the root in the dispatch path so shortcuts work before any
        // file is open.
        this.focus_active(window, cx);
        this
    }

    pub fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None
        }));
        cx.notify();
    }

    pub fn info(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.show_notice(Notice::info(message), cx);
    }

    pub fn error(&mut self, message: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.show_notice(Notice::error(message), cx);
    }

    /// The active pane's active editor (previews resolve to their source).
    pub fn active_editor(&self) -> Option<Entity<EditorView>> {
        self.panes
            .get(self.active_pane)?
            .active_item()
            .map(|item| item.editor())
    }

    /// Every open editor, across panes, without duplicates.
    pub fn all_editors(&self) -> Vec<Entity<EditorView>> {
        let mut out: Vec<Entity<EditorView>> = Vec::new();
        for pane in &self.panes {
            for item in &pane.items {
                if let PaneItem::Editor(editor) = item
                    && !out.iter().any(|e| e.entity_id() == editor.entity_id())
                {
                    out.push(editor.clone());
                }
            }
        }
        out
    }

    fn on_run_command(&mut self, action: &RunCommand, window: &mut Window, cx: &mut Context<Self>) {
        self.run_command(&action.name, window, cx);
    }

    /// Runs any command by name: registry handlers first, then editor
    /// commands on the active editor, then tree-view commands.
    pub fn run_command(&mut self, name: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(handler) = self.registry.get(name) {
            handler(self, window, cx);
            return;
        }
        if name.starts_with("editor:") {
            if let Some(editor) = self.active_editor() {
                if name == "editor:cancel" {
                    self.dismiss_transient(window, cx);
                }
                editor.update(cx, |editor, cx| editor.run_command(name, window, cx));
            } else if name == "editor:cancel" {
                self.dismiss_transient(window, cx);
            }
            return;
        }
        if name.starts_with("tree-view:")
            && let Some(tree) = self.tree.clone()
        {
            tree.update(cx, |tree, cx| tree.run_command(name, window, cx));
            return;
        }
        self.info(format!("Unknown command `{name}`"), cx);
    }

    /// Escape with nothing else to do: close the find bar / panel.
    fn dismiss_transient(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.find_visible {
            self.hide_find_bar(window, cx);
        } else if self.project_find_visible {
            self.project_find_visible = false;
            cx.notify();
        }
    }

    /// "file — folder — Ferrum Editor", with a dot when unsaved.
    fn update_window_title(&mut self, window: &mut Window, cx: &App) {
        let mut parts: Vec<String> = Vec::new();
        if let Some(editor) = self.active_editor() {
            let editor = editor.read(cx);
            let dirty = if editor.is_dirty() { " •" } else { "" };
            parts.push(format!("{}{dirty}", editor.title()));
        }
        if let Some(name) = self.root.as_deref().and_then(|r| r.file_name()) {
            parts.push(name.to_string_lossy().to_string());
        }
        parts.push("Ferrum Editor".into());
        let title = parts.join(" — ");
        if title != self.window_title {
            window.set_window_title(&title);
            self.window_title = title;
        }
    }

    fn on_drag_splitter(
        &mut self,
        event: &DragMoveEvent<ferrum_ui::DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx);
        match drag.id.as_ref() {
            "tree-splitter" => {
                self.tree_width =
                    (event.event.position.x - event.bounds.left()).clamp(px(140.), px(600.));
            }
            "project-find-splitter" => {
                self.project_find_height = (event.bounds.bottom()
                    - event.event.position.y
                    - px(STATUS_BAR_HEIGHT)
                    - self.terminal_height(cx))
                .clamp(px(120.), px(700.));
            }
            "terminal-splitter" => {
                let height = event.bounds.bottom() - event.event.position.y - px(STATUS_BAR_HEIGHT);
                let available = f32::from(event.bounds.size.height);
                self.terminal
                    .update(cx, |p, _| p.state.set_height(f32::from(height), available));
            }
            _ => return,
        }
        cx.notify();
    }
}

/// Height of the status bar below the panels, for splitter maths.
const STATUS_BAR_HEIGHT: f32 = 26.0;

impl Workspace {
    /// Height the open terminal panel takes (0 when hidden).
    fn terminal_height(&self, cx: &App) -> Pixels {
        let panel = self.terminal.read(cx);
        if panel.is_open() {
            px(panel.state.height) + px(4.)
        } else {
            px(0.)
        }
    }
}

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_window_title(window, cx);
        let colors = cx.theme().colors().clone();
        let tree = self.tree.clone().filter(|_| self.tree_visible);
        let (terminal_open, terminal_height) = {
            let panel = self.terminal.read(cx);
            (panel.is_open(), panel.state.height)
        };
        let center = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(div().flex_1().min_h_0().child(self.render_layout(
                &self.layout.clone(),
                window,
                cx,
            )))
            .when(self.find_visible, |this| this.child(self.find_bar.clone()))
            .when(self.project_find_visible, |this| {
                this.child(splitter("project-find-splitter", Axis::Vertical, cx))
                    .child(
                        div()
                            .flex_none()
                            .h(self.project_find_height)
                            .child(self.project_find.clone()),
                    )
            })
            .when(terminal_open, |this| {
                this.child(splitter("terminal-splitter", Axis::Vertical, cx))
                    .child(
                        div()
                            .flex_none()
                            .h(px(terminal_height))
                            .child(self.terminal.clone()),
                    )
            });
        v_flex()
            .relative()
            .size_full()
            .bg(colors.background)
            .text_color(colors.text)
            .track_focus(&self.focus_handle)
            .key_context("FerrumWorkspace")
            .on_action(cx.listener(Self::on_run_command))
            .on_drag_move(cx.listener(Self::on_drag_splitter))
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .when_some(tree, |this, tree| {
                        this.child(div().flex_none().h_full().w(self.tree_width).child(tree))
                            .child(splitter("tree-splitter", Axis::Horizontal, cx))
                    })
                    .child(center),
            )
            .child(self.render_status_bar(cx))
            .when_some(self.picker.clone(), |this, picker| {
                this.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .justify_center()
                        .pt(px(64.))
                        .occlude()
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, window, cx| this.close_picker(window, cx)),
                        )
                        .child(
                            div()
                                .on_mouse_down(gpui::MouseButton::Left, |_, _, cx| {
                                    cx.stop_propagation()
                                })
                                .child(picker),
                        ),
                )
            })
            .when_some(self.render_dialog(window, cx), |this, dialog| {
                this.child(dialog)
            })
            .when_some(self.notice.as_ref(), |this, notice| {
                this.child(render_notice(
                    notice,
                    cx.listener(|this, _, _, cx| {
                        this.notice = None;
                        cx.notify();
                    }),
                    cx,
                ))
            })
    }
}

#[cfg(test)]
impl Workspace {
    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    pub fn project_find(&self) -> &Entity<ProjectFind> {
        &self.project_find
    }

    pub fn terminal(&self) -> &Entity<TerminalPanel> {
        &self.terminal
    }

    pub fn tree(&self) -> Option<Entity<TreeView>> {
        self.tree.clone()
    }

    pub fn picker_open(&self) -> bool {
        self.picker.is_some()
    }
}

//! The window's root view: tree view, split panes of tabs, find bar,
//! find-in-files panel, the tool panes docked around them (Git, Agent,
//! Term, CI, Logs, Debug — see `tools`), overlays (palette, file finder,
//! go to line, theme selectors), dialogs and the status bar. Behaviour is
//! split by topic into the submodules.

mod agents;
mod ci;
mod commands;
mod debug;
mod dialogs;
mod drop;
mod edit_preview;
mod files;
mod git;
mod logs;
mod lsp;
mod lsp_edits;
mod overlays;
mod panes;
mod quit;
mod session;
mod status_bar;
mod tasks;
mod terminal;
mod themes;
mod tools;
mod top_bar;
mod watcher;

use std::path::PathBuf;

use ferrum_ui::prelude::*;
use ferrum_ui::{Notice, ProjectBar, ProjectBarEvent, ToggleProjectSwitcher, render_notice};
use gpui::{Axis, DragMoveEvent};

pub use panes::{Pane, PaneItem, PaneNode};

use crate::actions::RunCommand;
use crate::editor::EditorView;
use crate::find_bar::FindBar;
use crate::picker::{Picker, PickerKind};
use crate::project_find::ProjectFind;
use crate::registry::CommandRegistry;
use crate::tree_view::TreeView;

pub use dialogs::Dialog;
pub(crate) use terminal::find_on_path;
pub use tools::Tools;

use ferrumeditor_core::tool_layout::{Dock, ToolKind};

pub struct Workspace {
    focus_handle: FocusHandle,
    pub root: Option<PathBuf>,
    /// The shared project bar, inline under the sidebar's title row (name,
    /// branch, switcher).
    project_bar: Entity<ProjectBar>,
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
    /// The tool panes and their layout.
    tools: Tools,
    /// Theme / icon theme in use when a selector opened (restored on Escape).
    theme_preview: Option<(PickerKind, String)>,
    dialog: Option<Dialog>,
    notice: Option<Notice>,
    notice_task: Option<Task<()>>,
    watcher: Option<watcher::Watcher>,
    file_list: Option<std::sync::Arc<Vec<PathBuf>>>,
    file_list_task: Option<Task<()>>,
    window_title: String,
    /// Places to return to after go to definition.
    nav_back: Vec<lsp::NavEntry>,
    nav_forward: Vec<lsp::NavEntry>,
    /// Items for the next Locations / Code Actions picker.
    location_items: Vec<crate::picker::PickerItem>,
    /// Code actions offered in the picker, and the editor they run on.
    code_actions: Option<(
        Entity<EditorView>,
        Vec<ferrumeditor_core::lsp::features::CodeAction>,
    )>,
    /// For tasks:rerun-last.
    last_task: Option<ferrumeditor_core::tasks::Task>,
    /// The CI pane's view, once opened (see `ci.rs`).
    ci: Option<Entity<crate::ci::CiView>>,
    /// Services the CI page starts with (tests inject fakes).
    ci_services: Option<crate::ci::Services>,
    /// The Logs pane's view, once opened (see `logs.rs`).
    logs: Option<Entity<crate::logs::LogsView>>,
    /// Options the Logs page starts with (tests use temp files).
    logs_options: Option<crate::logs::Options>,
    debugger: Entity<crate::debug::Debugger>,
    debug_panel: Entity<crate::debug::DebugPanel>,
    _subscriptions: Vec<Subscription>,
}

impl Workspace {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let find_bar = cx.new(|cx| FindBar::new(window, cx));
        let project_find = cx.new(|cx| ProjectFind::new(window, cx));
        let project_bar = cx.new(|cx| ProjectBar::new("ferrumeditor", window, cx).inline(true));
        let debugger = cx.new(crate::debug::Debugger::new);
        let debug_panel = cx.new(|cx| crate::debug::DebugPanel::new(debugger.clone(), window, cx));
        let mut subscriptions = vec![
            cx.subscribe_in(&find_bar, window, Self::on_find_bar_event),
            cx.subscribe_in(&project_find, window, Self::on_project_find_event),
            // Files may change while the window is in the background.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.check_disk_changes(window, cx);
                    // With several windows, servers start for the project
                    // of the window in front.
                    if let Some(store) = crate::lsp_store::LspStore::global(cx) {
                        let project = this.root.clone();
                        store.update(cx, |store, _| store.set_project(project));
                    }
                }
            }),
            cx.subscribe_in(&debugger, window, Self::on_debugger_event),
            cx.subscribe_in(&debug_panel, window, Self::on_debug_panel_event),
            // A project picked in the switcher opens like Open Folder.
            cx.subscribe_in(&project_bar, window, |this, _, event, window, cx| {
                let ProjectBarEvent::Open(path) = event;
                this.set_root(path.clone(), window, cx);
            }),
        ];
        // Problem counts and server state live in the status bar.
        if let Some(store) = crate::lsp_store::LspStore::global(cx) {
            subscriptions.push(cx.observe(&store, |_, _, cx| cx.notify()));
            subscriptions.push(cx.subscribe(&store, |this, _, event, cx| {
                this.on_lsp_store_event(event, cx)
            }));
        }
        let mut this = Self {
            focus_handle: cx.focus_handle(),
            root: None,
            project_bar,
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
            tools: Tools::new(),
            theme_preview: None,
            dialog: None,
            notice: None,
            notice_task: None,
            watcher: None,
            file_list: None,
            file_list_task: None,
            window_title: String::new(),
            nav_back: Vec::new(),
            nav_forward: Vec::new(),
            location_items: Vec::new(),
            code_actions: None,
            last_task: None,
            ci: None,
            ci_services: None,
            logs: None,
            logs_options: None,
            debugger,
            debug_panel,
            _subscriptions: subscriptions,
        };
        // Tool pane layouts of every project (also when not restoring).
        this.tools.saved = this.saved_tool_layouts(cx);
        this.load_tool_layout(cx);
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
            .and_then(|item| item.editor())
    }

    /// The file shown by the active pane's active editor or viewer.
    pub fn active_path(&self, cx: &App) -> Option<std::path::PathBuf> {
        self.panes
            .get(self.active_pane)?
            .active_item()
            .and_then(|item| item.path(cx))
    }

    /// Every open image / binary-file viewer.
    pub fn all_viewers(&self) -> Vec<Entity<crate::file_viewer::FileViewer>> {
        let mut out: Vec<Entity<crate::file_viewer::FileViewer>> = Vec::new();
        for pane in &self.panes {
            for item in &pane.items {
                if let PaneItem::Viewer(viewer) = item
                    && !out.contains(viewer)
                {
                    out.push(viewer.clone());
                }
            }
        }
        out
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

    /// "file — folder — FerrumIDE", with a dot when unsaved.
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
        parts.push("FerrumIDE".into());
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
        let id = drag.id.clone();
        match id.as_ref() {
            "tree-splitter" => {
                self.tree_width =
                    (event.event.position.x - event.bounds.left()).clamp(px(140.), px(600.));
            }
            "project-find-splitter" => {
                self.project_find_height = (event.bounds.bottom()
                    - event.event.position.y
                    - px(STATUS_BAR_HEIGHT)
                    - self.bottom_dock_height())
                .clamp(px(120.), px(700.));
            }
            other => {
                // The tree and its splitter, or the main column's padding.
                let sidebar = if self.sidebar_shown() {
                    self.tree_width + ferrum_ui::PANE_GAP
                } else {
                    ferrum_ui::PANE_GAP
                };
                if !self.on_drag_dock_splitter(other, event, sidebar) {
                    return;
                }
            }
        }
        cx.notify();
    }

    fn sidebar_shown(&self) -> bool {
        self.tree_visible && self.root.is_some()
    }

    /// The tree view on the left (Git lives in its own pane).
    fn render_sidebar(&self) -> Option<AnyElement> {
        if !self.sidebar_shown() {
            return None;
        }
        self.tree.clone().map(IntoElement::into_any_element)
    }

    /// Height the open bottom dock takes (0 when it is empty).
    fn bottom_dock_height(&self) -> Pixels {
        if self.tools.layout.visible(Dock::Bottom).is_empty() {
            px(0.)
        } else {
            px(self.tools.layout.dock_size(Dock::Bottom)) + px(4.)
        }
    }
}

/// Height of the status bar below the panels, for splitter maths.
const STATUS_BAR_HEIGHT: f32 = 26.0;

impl Focusable for Workspace {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Workspace {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.update_window_title(window, cx);
        self.sync_chat_visibility(cx);
        let colors = cx.theme().colors().clone();
        if !cx.has_active_drag() {
            self.tools.dragging = false;
        }
        let sidebar = self.render_sidebar_column(window, cx);
        // With one pane its tabs sit in the editor card's header.
        let header_tabs = match self.layout {
            PaneNode::Leaf(id) => self.pane_index(id).map(|index| self.pane_tabs(index, cx)),
            PaneNode::Split { .. } => None,
        };
        let header = self.render_editor_header(header_tabs, window, cx);
        let content = self
            .render_layout(&self.layout.clone(), cx)
            .into_any_element();
        let bottom = self.render_dock(Dock::Bottom, window, cx);
        let bottom_height = px(self.tools.layout.dock_size(Dock::Bottom));
        // The editor area is a card like the tool panes around it, lighter
        // unless a tool pane has focus.
        let editor_active = !ToolKind::ALL
            .into_iter()
            .any(|kind| self.tool_open(kind) && self.tool_focused(kind, window, cx));
        let center = v_flex()
            .flex_1()
            .min_w_0()
            .h_full()
            .child(
                ferrum_ui::pane_card(editor_active, cx)
                    .flex_1()
                    .child(header)
                    .child(div().flex_1().min_h_0().child(content))
                    .when(self.find_visible, |this| this.child(self.find_bar.clone())),
            )
            .when(self.project_find_visible, |this| {
                this.child(ferrum_ui::gap_splitter(
                    "project-find-splitter",
                    Axis::Vertical,
                    cx,
                ))
                .child(
                    ferrum_ui::pane_frame(cx)
                        .flex_none()
                        .h(self.project_find_height)
                        .child(self.project_find.clone()),
                )
            })
            .when_some(bottom, |this, dock| {
                this.child(ferrum_ui::gap_splitter(
                    "dock-splitter-bottom",
                    Axis::Vertical,
                    cx,
                ))
                .child(div().flex_none().h(bottom_height).child(dock))
            });
        let side = |dock: Dock, this: &Self, window: &Window, cx: &mut Context<Self>| {
            let width = px(this.tools.layout.dock_size(dock));
            this.render_dock(dock, window, cx)
                .map(|d| div().flex_none().h_full().w(width).child(d))
        };
        let left = side(Dock::Left, self, window, cx);
        let right = side(Dock::Right, self, window, cx);
        let drop_zones = self.render_drop_zones(cx);
        let sidebar_shown = sidebar.is_some();
        // The sidebar sits on the window backdrop; the cards to its right.
        v_flex()
            .relative()
            .size_full()
            .text_color(colors.text)
            .track_focus(&self.focus_handle)
            .key_context("FerrumWorkspace")
            .on_action(cx.listener(Self::on_run_command))
            .on_action(cx.listener(|this, _: &ToggleProjectSwitcher, window, cx| {
                this.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
            }))
            .on_drag_move(cx.listener(Self::on_drag_splitter))
            // Files dropped outside the panes (tree, tool panes): the active pane.
            .on_drop(
                cx.listener(|this, paths: &gpui::ExternalPaths, window, cx| {
                    this.open_dropped(paths, None, window, cx)
                }),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .when_some(sidebar, |this, sidebar| {
                        this.child(sidebar).child(ferrum_ui::gap_splitter(
                            "tree-splitter",
                            Axis::Horizontal,
                            cx,
                        ))
                    })
                    .child(
                        h_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .pt(ferrum_ui::PANE_GAP)
                            .pr(ferrum_ui::PANE_GAP)
                            .when(!sidebar_shown, |this| this.pl(ferrum_ui::PANE_GAP))
                            .when_some(left, |this, dock| {
                                this.child(dock).child(ferrum_ui::gap_splitter(
                                    "dock-splitter-left",
                                    Axis::Horizontal,
                                    cx,
                                ))
                            })
                            .child(center)
                            .when_some(right, |this, dock| {
                                this.child(ferrum_ui::gap_splitter(
                                    "dock-splitter-right",
                                    Axis::Horizontal,
                                    cx,
                                ))
                                .child(dock)
                            }),
                    ),
            )
            .child(self.render_status_bar(cx))
            .children(drop_zones)
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
    pub fn project_bar(&self) -> &Entity<ProjectBar> {
        &self.project_bar
    }

    pub fn has_dialog(&self) -> bool {
        self.dialog.is_some()
    }

    /// Whether a rename preview is open.
    pub fn edit_preview_open(&self) -> bool {
        matches!(self.dialog, Some(Dialog::EditPreview(_)))
    }

    pub fn project_find(&self) -> &Entity<ProjectFind> {
        &self.project_find
    }

    pub fn project_find_visible(&self) -> bool {
        self.project_find_visible
    }

    pub fn tree(&self) -> Option<Entity<TreeView>> {
        self.tree.clone()
    }

    pub fn picker_open(&self) -> bool {
        self.picker.is_some()
    }

    pub fn debugger(&self) -> &Entity<crate::debug::Debugger> {
        &self.debugger
    }

    pub fn notice_message(&self) -> Option<SharedString> {
        self.notice.as_ref().map(|n| n.message.clone())
    }

    /// The active pane's active item, if it is a viewer.
    pub fn active_viewer(&self) -> Option<Entity<crate::file_viewer::FileViewer>> {
        match self.panes.get(self.active_pane)?.active_item()? {
            PaneItem::Viewer(viewer) => Some(viewer.clone()),
            _ => None,
        }
    }

    /// Index of the active pane (in split order) and how many there are.
    pub fn pane_position(&self) -> (usize, usize) {
        (self.active_pane, self.panes.len())
    }

    pub fn active_pane_tab_count(&self) -> usize {
        self.panes
            .get(self.active_pane)
            .map_or(0, |p| p.items.len())
    }
}

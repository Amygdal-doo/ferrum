//! Session save / restore: the open folder, pane layout, tabs, cursor and
//! scroll positions (`~/.ferrum/editor/session.json`).

use ferrum_ui::prelude::*;
use ferrumeditor_core::session::{self, PaneLayout, PaneSession, Session, TabSession};

use super::{Pane, PaneItem, PaneNode, Workspace};
use crate::settings::Settings;

impl Workspace {
    fn session_path(cx: &App) -> Option<std::path::PathBuf> {
        cx.global::<Settings>()
            .files
            .as_ref()
            .map(|f| f.session.clone())
    }

    /// The per-project tool pane layouts stored in the session file.
    pub(super) fn saved_tool_layouts(
        &self,
        cx: &App,
    ) -> std::collections::BTreeMap<String, ferrumeditor_core::tool_layout::ToolLayout> {
        Self::session_path(cx)
            .map(|path| session::load(&path).tools)
            .unwrap_or_default()
    }

    pub fn save_session(&self, cx: &App) {
        let Some(path) = Self::session_path(cx) else {
            return;
        };
        let order = self.layout.leaves();
        let active_id = self.panes.get(self.active_pane).map(|p| p.id);
        let session = Session {
            folder: self.root.clone(),
            layout: self.layout_session(&self.layout, cx),
            active_pane: order
                .iter()
                .position(|id| Some(*id) == active_id)
                .unwrap_or(0),
            tree_visible: self.tree_visible,
            tree_width: f32::from(self.tree_width),
            // Other windows' / earlier runs' projects keep their layouts.
            tools: {
                let mut tools = session::load(&path).tools;
                tools.extend(self.tool_layouts());
                tools
            },
        };
        if let Err(error) = session::save(&path, &session) {
            eprintln!("ferrumeditor: could not save the session: {error}");
        }
    }

    fn layout_session(&self, node: &PaneNode, cx: &App) -> PaneLayout {
        match node {
            PaneNode::Leaf(id) => PaneLayout::Pane(
                self.pane_index(*id)
                    .map_or_else(PaneSession::default, |i| pane_session(&self.panes[i], cx)),
            ),
            PaneNode::Split { axis, children } => PaneLayout::Split {
                axis: *axis,
                children: children
                    .iter()
                    .map(|c| self.layout_session(c, cx))
                    .collect(),
            },
        }
    }

    /// Restores the last session. Returns false when there was none.
    pub fn restore_session(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(path) = Self::session_path(cx) else {
            return false;
        };
        if !path.exists() {
            return false;
        }
        let saved = session::load(&path);
        self.tools.saved = saved.tools.clone();
        self.load_tool_layout(cx);
        if let Some(folder) = saved.folder.clone().filter(|f| f.is_dir()) {
            self.set_root(folder, window, cx);
        }
        self.tree_visible = saved.tree_visible;
        self.tree_width = px(saved.tree_width.clamp(140., 600.));
        self.panes.clear();
        self.next_pane_id = 0;
        self.layout = self.restore_node(&saved.layout, window, cx);
        if self.panes.is_empty() {
            self.panes.push(Pane::new(self.next_pane_id));
            self.layout = PaneNode::Leaf(self.next_pane_id);
            self.next_pane_id += 1;
        }
        let order = self.layout.leaves();
        self.active_pane = order
            .get(saved.active_pane)
            .and_then(|id| self.pane_index(*id))
            .unwrap_or(0);
        self.sync_tool_views(window, cx);
        self.focus_active(window, cx);
        cx.notify();
        true
    }

    fn restore_node(
        &mut self,
        layout: &PaneLayout,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> PaneNode {
        match layout {
            PaneLayout::Pane(saved) => {
                let id = self.next_pane_id;
                self.next_pane_id += 1;
                self.panes.push(Pane::new(id));
                self.active_pane = self.panes.len() - 1;
                for tab in &saved.tabs {
                    if !tab.path.is_file() {
                        continue;
                    }
                    if let Some(editor) = self.open_file(&tab.path, window, cx) {
                        editor.update(cx, |e, _| {
                            e.state.go_to_point(tab.line, tab.column);
                            e.set_scroll_line(tab.scroll_line);
                        });
                    }
                }
                let pane = &mut self.panes[self.active_pane];
                pane.active = saved.active.min(pane.items.len().saturating_sub(1));
                PaneNode::Leaf(id)
            }
            PaneLayout::Split { axis, children } => {
                let children: Vec<PaneNode> = children
                    .iter()
                    .map(|c| self.restore_node(c, window, cx))
                    .collect();
                match children.len() {
                    0 => self.restore_node(&PaneLayout::default(), window, cx),
                    1 => children.into_iter().next().unwrap_or(PaneNode::Leaf(0)),
                    _ => PaneNode::Split {
                        axis: *axis,
                        children,
                    },
                }
            }
        }
    }
}

fn pane_session(pane: &Pane, cx: &App) -> PaneSession {
    let mut tabs = Vec::new();
    let mut active = 0;
    for (i, item) in pane.items.iter().enumerate() {
        let tab = match item {
            PaneItem::Editor(editor) => {
                let editor = editor.read(cx);
                let Some(path) = editor.path.clone() else {
                    continue;
                };
                let point = editor.cursor_point();
                TabSession {
                    path,
                    line: point.row,
                    column: point.col,
                    scroll_line: editor.scroll_line(),
                }
            }
            // Images reopen in a viewer.
            PaneItem::Viewer(viewer) => TabSession {
                path: viewer.read(cx).path.clone(),
                line: 0,
                column: 0,
                scroll_line: 0,
            },
            PaneItem::Preview(..) => continue,
        };
        if i == pane.active {
            active = tabs.len();
        }
        tabs.push(tab);
    }
    PaneSession { tabs, active }
}

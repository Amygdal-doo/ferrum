//! Panes (tab stacks) and their split layout: adding / activating /
//! closing items, splitting right or down, moving focus, rendering tabs.

use ferrum_ui::components::{IconButton, IconName, IconSize};
use ferrum_ui::prelude::*;
use ferrumeditor_core::session::SplitAxis;

use super::Workspace;
use crate::editor::{EditorEvent, EditorView};
use crate::file_viewer::{FileViewer, FileViewerEvent};
use crate::markdown_preview::MarkdownPreview;

#[derive(Clone)]
pub enum PaneItem {
    Editor(Entity<EditorView>),
    /// A Markdown preview and the editor it renders.
    Preview(Entity<MarkdownPreview>, Entity<EditorView>),
    /// An image, or another file that is not text.
    Viewer(Entity<FileViewer>),
}

impl PaneItem {
    /// The editor this item shows (a preview's source editor); viewers
    /// have none.
    pub fn editor(&self) -> Option<Entity<EditorView>> {
        match self {
            PaneItem::Editor(editor) => Some(editor.clone()),
            PaneItem::Preview(_, source) => Some(source.clone()),
            PaneItem::Viewer(_) => None,
        }
    }

    /// The file an editor or viewer shows.
    pub fn path(&self, cx: &App) -> Option<std::path::PathBuf> {
        match self {
            PaneItem::Editor(editor) => editor.read(cx).path.clone(),
            PaneItem::Preview(..) => None,
            PaneItem::Viewer(viewer) => Some(viewer.read(cx).path.clone()),
        }
    }

    pub fn entity_id(&self) -> gpui::EntityId {
        match self {
            PaneItem::Editor(e) => e.entity_id(),
            PaneItem::Preview(p, _) => p.entity_id(),
            PaneItem::Viewer(v) => v.entity_id(),
        }
    }
}

pub struct Pane {
    pub id: usize,
    pub items: Vec<PaneItem>,
    pub active: usize,
}

impl Pane {
    pub fn new(id: usize) -> Self {
        Self {
            id,
            items: Vec::new(),
            active: 0,
        }
    }

    pub fn active_item(&self) -> Option<&PaneItem> {
        self.items.get(self.active)
    }
}

#[derive(Clone, Debug)]
pub enum PaneNode {
    Leaf(usize),
    Split {
        axis: SplitAxis,
        children: Vec<PaneNode>,
    },
}

impl PaneNode {
    /// Pane ids in depth-first order.
    pub fn leaves(&self) -> Vec<usize> {
        match self {
            PaneNode::Leaf(id) => vec![*id],
            PaneNode::Split { children, .. } => {
                children.iter().flat_map(PaneNode::leaves).collect()
            }
        }
    }

    /// Replaces leaf `target` with a split of `target` and `new`.
    fn split(&mut self, target: usize, new: usize, axis: SplitAxis) -> bool {
        match self {
            PaneNode::Leaf(id) if *id == target => {
                *self = PaneNode::Split {
                    axis,
                    children: vec![PaneNode::Leaf(target), PaneNode::Leaf(new)],
                };
                true
            }
            PaneNode::Leaf(_) => false,
            PaneNode::Split {
                axis: own,
                children,
            } => {
                if let Some(index) = children
                    .iter()
                    .position(|c| matches!(c, PaneNode::Leaf(id) if *id == target))
                    && *own == axis
                {
                    children.insert(index + 1, PaneNode::Leaf(new));
                    return true;
                }
                children.iter_mut().any(|c| c.split(target, new, axis))
            }
        }
    }

    /// Removes leaf `target`, collapsing single-child splits.
    fn remove(&mut self, target: usize) {
        if let PaneNode::Split { children, .. } = self {
            children.retain(|c| !matches!(c, PaneNode::Leaf(id) if *id == target));
            for child in children.iter_mut() {
                child.remove(target);
            }
            if children.len() == 1 {
                let only = children.remove(0);
                *self = only;
            }
        }
    }
}

impl Workspace {
    pub(super) fn pane_index(&self, id: usize) -> Option<usize> {
        self.panes.iter().position(|p| p.id == id)
    }

    /// Adds (or activates, if already there) an item in the active pane.
    pub(super) fn add_item(&mut self, item: PaneItem, window: &mut Window, cx: &mut Context<Self>) {
        let pane = &mut self.panes[self.active_pane];
        if let Some(index) = pane
            .items
            .iter()
            .position(|i| i.entity_id() == item.entity_id())
        {
            pane.active = index;
        } else {
            let at = (pane.active + 1).min(pane.items.len());
            pane.items.insert(at, item.clone());
            pane.active = at;
            match &item {
                PaneItem::Editor(editor) => {
                    self.watch_editor(editor, window, cx);
                    self.sync_debug_marks_for(editor, cx);
                }
                PaneItem::Viewer(viewer) => self.watch_viewer(viewer, window, cx),
                PaneItem::Preview(..) => {}
            }
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    fn watch_editor(
        &mut self,
        editor: &Entity<EditorView>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe_in(
            editor,
            window,
            |this: &mut Self, editor, event: &EditorEvent, window, cx| {
                if let EditorEvent::ToggleBreakpoint(line) = event {
                    this.toggle_breakpoint_at(editor, *line, cx);
                    return;
                }
                if matches!(
                    event,
                    EditorEvent::Navigate { .. }
                        | EditorEvent::Notice(_)
                        | EditorEvent::ApplyEdit { .. }
                        | EditorEvent::PreviewEdit { .. }
                        | EditorEvent::References { .. }
                        | EditorEvent::CodeActions(_)
                ) {
                    this.on_editor_lsp_event(editor, event, window, cx);
                    return;
                }
                if *event == EditorEvent::Focused
                    && let Some(index) = this.panes.iter().position(|p| {
                        p.items
                            .iter()
                            .any(|i| matches!(i, PaneItem::Editor(e) if e == editor))
                    })
                {
                    this.active_pane = index;
                    if let Some(pos) = this.panes[index]
                        .items
                        .iter()
                        .position(|i| matches!(i, PaneItem::Editor(e) if e == editor))
                    {
                        this.panes[index].active = pos;
                    }
                    this.sync_tree_selection(cx);
                }
                cx.notify();
            },
        );
        self._subscriptions.push(subscription);
    }

    fn watch_viewer(
        &mut self,
        viewer: &Entity<FileViewer>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let subscription = cx.subscribe_in(
            viewer,
            window,
            |this: &mut Self, _, event: &FileViewerEvent, window, cx| match event {
                FileViewerEvent::OpenAsText(path) => {
                    this.open_text_file(path, window, cx);
                }
            },
        );
        self._subscriptions.push(subscription);
    }

    pub(super) fn focus_active(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self
            .panes
            .get(self.active_pane)
            .and_then(|p| p.active_item())
            .cloned()
        {
            Some(PaneItem::Editor(editor)) => editor.update(cx, |e, cx| e.focus(window, cx)),
            Some(PaneItem::Preview(preview, _)) => window.focus(&preview.focus_handle(cx), cx),
            Some(PaneItem::Viewer(viewer)) => window.focus(&viewer.focus_handle(cx), cx),
            None => window.focus(&self.focus_handle, cx),
        }
        self.sync_tree_selection(cx);
    }

    pub(super) fn activate_item(
        &mut self,
        pane: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.active_pane = pane;
        if let Some(p) = self.panes.get_mut(pane) {
            p.active = index.min(p.items.len().saturating_sub(1));
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn cycle_tab(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let pane = &mut self.panes[self.active_pane];
        if pane.items.is_empty() {
            return;
        }
        let len = pane.items.len() as isize;
        pane.active = ((pane.active as isize + delta).rem_euclid(len)) as usize;
        self.focus_active(window, cx);
        cx.notify();
    }

    /// Removes an item without any prompt.
    pub(super) fn remove_item(
        &mut self,
        pane: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(p) = self.panes.get_mut(pane) else {
            return;
        };
        if index >= p.items.len() {
            return;
        }
        let removed = p.items.remove(index);
        if let Some(path) = removed.path(cx) {
            self.closed_tabs.push(path);
        }
        let p = &mut self.panes[pane];
        if p.active >= index && p.active > 0 {
            p.active -= 1;
        }
        // Previews of a closed editor go with it, unless it is open elsewhere.
        if let PaneItem::Editor(editor) = removed
            && !self.all_editors().contains(&editor)
        {
            for p in &mut self.panes {
                p.items
                    .retain(|i| !matches!(i, PaneItem::Preview(_, source) if *source == editor));
                p.active = p.active.min(p.items.len().saturating_sub(1));
            }
        }
        if self.panes[pane].items.is_empty() && self.panes.len() > 1 {
            self.close_pane(pane, window, cx);
            return;
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn split(&mut self, axis: SplitAxis, window: &mut Window, cx: &mut Context<Self>) {
        let id = self.next_pane_id;
        self.next_pane_id += 1;
        let current = self.panes[self.active_pane].id;
        let mut pane = Pane::new(id);
        if let Some(item @ (PaneItem::Editor(_) | PaneItem::Viewer(_))) =
            self.panes[self.active_pane].active_item().cloned()
        {
            pane.items.push(item);
        }
        self.panes.push(pane);
        if !self.layout.split(current, id, axis) {
            self.layout = PaneNode::Split {
                axis,
                children: vec![self.layout.clone(), PaneNode::Leaf(id)],
            };
        }
        self.active_pane = self.panes.len() - 1;
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn close_pane(&mut self, pane: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.panes.len() <= 1 {
            return;
        }
        let id = self.panes[pane].id;
        self.panes.remove(pane);
        self.layout.remove(id);
        let order = self.layout.leaves();
        self.active_pane = order
            .first()
            .and_then(|first| self.pane_index(*first))
            .unwrap_or(0);
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn focus_pane(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        let order = self.layout.leaves();
        let current = self.panes[self.active_pane].id;
        let Some(pos) = order.iter().position(|id| *id == current) else {
            return;
        };
        let next = order[((pos as isize + delta).rem_euclid(order.len() as isize)) as usize];
        if let Some(index) = self.pane_index(next) {
            self.active_pane = index;
            self.focus_active(window, cx);
            cx.notify();
        }
    }

    pub(super) fn render_layout(&self, node: &PaneNode, cx: &mut Context<Self>) -> AnyElement {
        match node {
            PaneNode::Leaf(id) => self.render_pane(*id, cx),
            PaneNode::Split { axis, children } => {
                let border = cx.theme().colors().border_variant;
                let mut container = match axis {
                    SplitAxis::Horizontal => h_flex(),
                    SplitAxis::Vertical => v_flex(),
                }
                .size_full();
                for (i, child) in children.iter().enumerate() {
                    if i > 0 {
                        let divider = div().flex_none().bg(border);
                        container = container.child(match axis {
                            SplitAxis::Horizontal => divider.w(px(1.)).h_full(),
                            SplitAxis::Vertical => divider.h(px(1.)).w_full(),
                        });
                    }
                    container = container.child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .child(self.render_layout(child, cx)),
                    );
                }
                container.into_any_element()
            }
        }
    }

    /// The tabs of pane `index`: rounded, the active one lighter.
    pub(super) fn pane_tabs(&self, index: usize, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let Some(pane) = self.panes.get(index) else {
            return Vec::new();
        };
        let id = pane.id;
        let accent = cx.theme().colors().text_accent;
        pane.items
            .iter()
            .enumerate()
            .map(|(i, item)| {
                let (title, dirty, path) = match item {
                    PaneItem::Editor(editor) => {
                        let editor = editor.read(cx);
                        (editor.title(), editor.is_dirty(), editor.path.clone())
                    }
                    PaneItem::Preview(preview, _) => (preview.read(cx).title(cx), false, None),
                    PaneItem::Viewer(viewer) => {
                        let viewer = viewer.read(cx);
                        (viewer.title(), false, Some(viewer.path.clone()))
                    }
                };
                let icon = path.and_then(|path| {
                    crate::file_icons::icon_element(
                        &path,
                        ferrumeditor_core::icons::EntryKind::File,
                        cx,
                    )
                });
                let active = i == pane.active;
                ferrum_ui::header_tab(("tab", id * 10_000 + i), active, cx)
                    .group("tab")
                    .pl_2()
                    .pr_0p5()
                    .gap_1()
                    .when_some(icon, |this, icon| this.child(icon))
                    .child(Label::new(title).size(LabelSize::Small).color(if active {
                        Color::Default
                    } else {
                        Color::Muted
                    }))
                    .child(
                        div()
                            .w(px(18.))
                            .flex()
                            .justify_center()
                            .when(dirty, |this| {
                                this.child(
                                    div()
                                        .size(px(7.))
                                        .rounded_full()
                                        .bg(accent)
                                        .group_hover("tab", |s| s.invisible()),
                                )
                            })
                            .child(
                                div()
                                    .when(dirty, |this| {
                                        this.absolute()
                                            .invisible()
                                            .group_hover("tab", |s| s.visible())
                                    })
                                    .child(
                                        IconButton::new(
                                            ("close-tab", id * 10_000 + i),
                                            IconName::Close,
                                        )
                                        .icon_size(IconSize::XSmall)
                                        .on_click(
                                            cx.listener(move |this, _, window, cx| {
                                                cx.stop_propagation();
                                                this.request_close_item(index, i, window, cx);
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.activate_item(index, i, window, cx)
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// One pane: its tab bar (unless the tabs are in the editor card's
    /// header, as with a single pane) and its active item.
    fn render_pane(&self, id: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(index) = self.pane_index(id) else {
            return div().into_any_element();
        };
        let pane = &self.panes[index];
        let colors = cx.theme().colors().clone();
        let own_tab_bar = !matches!(self.layout, PaneNode::Leaf(_)) && !pane.items.is_empty();
        let tabs = if own_tab_bar {
            self.pane_tabs(index, cx)
        } else {
            Vec::new()
        };
        let content = match pane.active_item() {
            Some(PaneItem::Editor(editor)) => editor.clone().into_any_element(),
            Some(PaneItem::Preview(preview, _)) => preview.clone().into_any_element(),
            Some(PaneItem::Viewer(viewer)) => viewer.clone().into_any_element(),
            None => empty_state(
                "No open files",
                "⌘P to find a file · ⌘⇧P for commands · ⌘N for a new file",
            )
            .into_any_element(),
        };
        v_flex()
            .size_full()
            .on_mouse_down(
                gpui::MouseButton::Left,
                cx.listener(move |this, _, _, cx| {
                    if this.active_pane != index {
                        this.active_pane = index;
                        cx.notify();
                    }
                }),
            )
            .when(own_tab_bar, |this| {
                this.child(
                    h_flex()
                        .id(("tab-bar", id))
                        .flex_none()
                        .h(px(34.))
                        .w_full()
                        .px_1()
                        .gap_1()
                        .overflow_x_scroll()
                        .border_b_1()
                        .border_color(colors.border_variant)
                        // The active pane's tabs at full strength.
                        .when(index != self.active_pane, |bar| bar.opacity(0.75))
                        .children(tabs),
                )
            })
            .child(div().flex_1().min_h_0().child(content))
            .into_any_element()
    }
}

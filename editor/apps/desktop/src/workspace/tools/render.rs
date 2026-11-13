//! Drawing the docks: each open tool as a pane with a small header (drag
//! handle, title, ⋯ menu with Move to Left / Right / Bottom and Close),
//! dividers between panes, and drop zones at the window edges while a
//! pane is dragged.

use ferrum_ui::components::{ContextMenu, IconButton, IconName, IconSize, PopoverMenu};
use ferrum_ui::prelude::*;
use ferrum_ui::{DraggedSplitter, splitter};
use ferrumeditor_core::tool_layout::{Dock, ToolKind};
use gpui::{Axis, DragMoveEvent, Stateful};

use super::super::Workspace;

/// What a dragged pane header carries (and draws under the pointer).
#[derive(Clone, Copy, Debug)]
pub struct DraggedTool(pub ToolKind);

impl Render for DraggedTool {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        div()
            .px_2()
            .py_1()
            .rounded_md()
            .bg(colors.element_selected)
            .border_1()
            .border_color(colors.border_focused)
            .text_sm()
            .child(self.0.title())
    }
}

pub fn tool_icon(kind: ToolKind) -> IconName {
    match kind {
        ToolKind::Git => IconName::GitBranch,
        ToolKind::Agent => IconName::Sparkle,
        ToolKind::Term => IconName::Terminal,
        ToolKind::Ci => IconName::PlayFilled,
        ToolKind::Logs => IconName::FileTree,
        ToolKind::Debug => IconName::Debug,
    }
}

fn dock_name(dock: Dock) -> &'static str {
    match dock {
        Dock::Left => "left",
        Dock::Right => "right",
        Dock::Bottom => "bottom",
    }
}

impl Workspace {
    /// The dock's panes, or `None` when none is open.
    pub(in crate::workspace) fn render_dock(
        &self,
        dock: Dock,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let visible = self.tools.layout.visible(dock);
        if visible.is_empty() {
            return None;
        }
        let vertical = dock.stacks_vertically();
        let count = visible.len();
        let mut column = if vertical { v_flex() } else { h_flex() };
        for (i, (kind, share)) in visible.into_iter().enumerate() {
            let pane = self
                .render_tool_pane(kind, dock, cx)
                .flex_basis(px(0.))
                .flex_grow(share)
                .when(!vertical, |d| d.min_w(px(kind.min_width())))
                .when(vertical, |d| d.min_h(px(80.)));
            column = column.child(pane);
            if i + 1 < count {
                let axis = if vertical {
                    Axis::Vertical
                } else {
                    Axis::Horizontal
                };
                column = column.child(splitter(
                    format!("tool-divider-{}-{i}", dock_name(dock)),
                    axis,
                    cx,
                ));
            }
        }
        Some(
            column
                .id(SharedString::from(format!("dock-{}", dock_name(dock))))
                .size_full()
                .overflow_hidden()
                .on_drag_move(cx.listener(
                    move |this, event: &DragMoveEvent<DraggedSplitter>, _, cx| {
                        this.on_drag_divider(dock, event, cx)
                    },
                ))
                .into_any_element(),
        )
    }

    fn on_drag_divider(
        &mut self,
        dock: Dock,
        event: &DragMoveEvent<DraggedSplitter>,
        cx: &mut Context<Self>,
    ) {
        let drag = event.drag(cx);
        let prefix = format!("tool-divider-{}-", dock_name(dock));
        let Some(boundary) = drag
            .id
            .strip_prefix(&prefix)
            .and_then(|i| i.parse::<usize>().ok())
        else {
            return;
        };
        let bounds = event.bounds;
        let position = if dock.stacks_vertically() {
            (event.event.position.y - bounds.top()) / bounds.size.height
        } else {
            (event.event.position.x - bounds.left()) / bounds.size.width
        };
        if position.is_finite() {
            self.tools.layout.drag_divider(dock, boundary, position);
            cx.notify();
        }
    }

    /// The edge splitters between the editor area and the docks.
    pub(in crate::workspace) fn on_drag_dock_splitter(
        &mut self,
        id: &str,
        event: &DragMoveEvent<DraggedSplitter>,
        sidebar_width: Pixels,
    ) -> bool {
        let bounds = event.bounds;
        let pos = event.event.position;
        let (dock, size) = match id {
            "dock-splitter-right" => (Dock::Right, bounds.right() - pos.x),
            "dock-splitter-left" => (Dock::Left, pos.x - bounds.left() - sidebar_width),
            "dock-splitter-bottom" => (
                Dock::Bottom,
                bounds.bottom() - pos.y - px(super::super::STATUS_BAR_HEIGHT),
            ),
            _ => return false,
        };
        self.tools.layout.set_dock_size(dock, f32::from(size));
        true
    }

    fn render_tool_pane(
        &self,
        kind: ToolKind,
        dock: Dock,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let colors = cx.theme().colors().clone();
        let id =
            |part: &str| SharedString::from(format!("tool-{}-{part}", kind.title().to_lowercase()));
        let header = h_flex()
            .id(id("header"))
            .flex_none()
            .h(px(26.))
            .pl_2()
            .pr_1()
            .gap_1()
            .bg(colors.panel_background)
            .border_b_1()
            .border_color(colors.border)
            .cursor_grab()
            .on_drag(DraggedTool(kind), {
                let weak = cx.weak_entity();
                move |dragged, _, _, cx| {
                    weak.update(cx, |w, cx| {
                        w.tools.dragging = true;
                        cx.notify();
                    })
                    .ok();
                    cx.new(|_| *dragged)
                }
            })
            .child(
                Icon::new(tool_icon(kind))
                    .size(IconSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Label::new(kind.title())
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(div().flex_1())
            .child(self.tool_menu(kind, dock, cx))
            .child(
                IconButton::new(id("close"), IconName::Close)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text(format!("Close {}", kind.title())))
                    .on_click(
                        cx.listener(move |this, _, window, cx| this.close_tool(kind, window, cx)),
                    ),
            );
        v_flex()
            .id(id("pane"))
            .debug_selector(move || format!("tool-pane-{}", kind.title().to_lowercase()))
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(colors.panel_background)
            .drag_over::<DraggedTool>(move |style, dragged, _, cx| {
                if dragged.0 == kind {
                    style
                } else {
                    style.bg(cx.theme().colors().drop_target_background)
                }
            })
            .on_drop(cx.listener(move |this, dragged: &DraggedTool, window, cx| {
                this.tools.dragging = false;
                this.move_tool_next_to(dragged.0, kind, window, cx);
            }))
            .child(header)
            .child(div().flex_1().min_h_0().child(self.tool_content(kind, cx)))
    }

    fn tool_menu(&self, kind: ToolKind, dock: Dock, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        PopoverMenu::new(SharedString::from(format!(
            "tool-menu-{}",
            kind.title().to_lowercase()
        )))
        .anchor(gpui::Anchor::TopRight)
        .trigger(
            IconButton::new(
                SharedString::from(format!("tool-more-{}", kind.title().to_lowercase())),
                IconName::Ellipsis,
            )
            .icon_size(IconSize::Small)
            .tooltip(Tooltip::text("Move or close")),
        )
        .menu(move |window, cx| {
            let weak = weak.clone();
            Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                for target in Dock::ALL {
                    if target == dock {
                        continue;
                    }
                    let weak = weak.clone();
                    menu = menu.entry(format!("Move to {}", target.title()), None, move |_, cx| {
                        weak.update(cx, |w, cx| w.move_tool(kind, target, cx)).ok();
                    });
                }
                if let Some((label, command)) = external_app(kind) {
                    let weak = weak.clone();
                    menu = menu.separator().entry(label, None, move |window, cx| {
                        weak.update(cx, |w, cx| w.run_command(command, window, cx))
                            .ok();
                    });
                }
                let weak = weak.clone();
                menu.separator().entry("Close", None, move |window, cx| {
                    weak.update(cx, |w, cx| w.close_tool(kind, window, cx)).ok();
                })
            }))
        })
    }

    fn tool_content(&self, kind: ToolKind, cx: &mut Context<Self>) -> AnyElement {
        let view: Option<AnyElement> = match kind {
            ToolKind::Git => self.tools.git.clone().map(IntoElement::into_any_element),
            ToolKind::Agent => self.tools.chat.clone().map(IntoElement::into_any_element),
            ToolKind::Term => self.tools.term.clone().map(IntoElement::into_any_element),
            ToolKind::Ci => self.ci.clone().map(IntoElement::into_any_element),
            ToolKind::Logs => self.logs.clone().map(IntoElement::into_any_element),
            ToolKind::Debug => Some(self.debug_panel.clone().into_any_element()),
        };
        view.unwrap_or_else(|| {
            let text = if self.root.is_none() {
                "Open a folder to use this tool (⌘⇧O)"
            } else {
                "Starting…"
            };
            div()
                .p_3()
                .text_sm()
                .text_color(cx.theme().colors().text_muted)
                .child(text)
                .into_any_element()
        })
    }

    /// Drop zones along the left, right and bottom edges while a pane is
    /// being dragged.
    pub(in crate::workspace) fn render_drop_zones(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !(self.tools.dragging && cx.has_active_drag()) {
            return None;
        }
        let colors = cx.theme().colors().clone();
        let zone = |dock: Dock| {
            div()
                .id(SharedString::from(format!("drop-{}", dock_name(dock))))
                .absolute()
                .flex()
                .items_center()
                .justify_center()
                .bg(colors.element_background.opacity(0.6))
                .border_1()
                .border_color(colors.border_focused)
                .text_sm()
                .text_color(colors.text_muted)
                .child(format!("Move to {}", dock.title()))
                .drag_over::<DraggedTool>(|style, _, _, cx| {
                    style.bg(cx.theme().colors().drop_target_background)
                })
                .on_drop(cx.listener(move |this, dragged: &DraggedTool, _, cx| {
                    this.tools.dragging = false;
                    this.move_tool(dragged.0, dock, cx);
                }))
        };
        Some(
            div()
                .absolute()
                .inset_0()
                .child(
                    zone(Dock::Left)
                        .left_0()
                        .top(px(40.))
                        .bottom(px(120.))
                        .w(px(96.)),
                )
                .child(
                    zone(Dock::Right)
                        .right_0()
                        .top(px(40.))
                        .bottom(px(120.))
                        .w(px(96.)),
                )
                .child(
                    zone(Dock::Bottom)
                        .left_0()
                        .right_0()
                        .bottom(px(28.))
                        .h(px(88.)),
                )
                .into_any_element(),
        )
    }
}

/// The ⋯ menu item that opens the stand-alone app, and its command.
pub(in crate::workspace) fn external_app(kind: ToolKind) -> Option<(&'static str, &'static str)> {
    match kind {
        ToolKind::Git => Some(("Open in FerrumGIT app", "git:open-in-ferrumgit")),
        ToolKind::Term => Some(("Open in FerrumTerm app", "terminal:open-in-ferrumterm")),
        _ => None,
    }
}

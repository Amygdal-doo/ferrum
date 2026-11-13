use crate::theme::ActiveTheme;
use gpui::{App, Axis, CursorStyle, IntoElement, SharedString, div, prelude::*, px};

/// Drag payload of a [`splitter`]. The parent container handles resizing with
/// `.on_drag_move(cx.listener(|this, event: &DragMoveEvent<DraggedSplitter>, ..| ..))`,
/// reading `event.drag(cx).id` to tell splitters apart and
/// `event.event.position` / `event.bounds` for the pointer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedSplitter {
    pub id: SharedString,
    pub axis: Axis,
}

impl Render for DraggedSplitter {
    fn render(&mut self, _: &mut gpui::Window, _: &mut gpui::Context<Self>) -> impl IntoElement {
        gpui::Empty
    }
}

/// A thin draggable divider. `Axis::Horizontal` resizes left/right panes
/// (vertical bar); `Axis::Vertical` resizes top/bottom panes.
pub fn splitter(id: impl Into<SharedString>, axis: Axis, cx: &App) -> impl IntoElement {
    let id = id.into();
    let colors = cx.theme().colors();
    let payload = DraggedSplitter {
        id: id.clone(),
        axis,
    };
    div()
        .id(gpui::ElementId::Name(id))
        .flex_none()
        .bg(colors.border)
        .hover(|style| style.bg(colors.border_focused))
        .map(|this| match axis {
            Axis::Horizontal => this.w(px(4.)).h_full().cursor(CursorStyle::ResizeLeftRight),
            Axis::Vertical => this.h(px(4.)).w_full().cursor(CursorStyle::ResizeUpDown),
        })
        .on_drag(payload, |payload, _, _, cx| cx.new(|_| payload.clone()))
}

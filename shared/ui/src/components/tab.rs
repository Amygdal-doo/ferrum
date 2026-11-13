//! [`Tab`] and [`TabBar`].

use gpui::{
    AnyElement, App, Div, ElementId, InteractiveElement, Interactivity, IntoElement, ParentElement,
    Pixels, RenderOnce, ScrollHandle, Stateful, StatefulInteractiveElement, Styled, Window, div,
    prelude::FluentBuilder, px,
};
use smallvec::SmallVec;

use super::Toggleable;
use super::traits::{StyledExt, rems_from_px};
use crate::theme::ActiveTheme;

/// Height of a tab bar and its tabs.
pub const TAB_HEIGHT: Pixels = px(32.);

/// One tab; add a label (and anything else) as children.
#[derive(IntoElement)]
pub struct Tab {
    div: Stateful<Div>,
    selected: bool,
    start_slot: Option<AnyElement>,
    end_slot: Option<AnyElement>,
    children: SmallVec<[AnyElement; 2]>,
}

impl Tab {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            div: div().id(id),
            selected: false,
            start_slot: None,
            end_slot: None,
            children: SmallVec::new(),
        }
    }

    pub fn start_slot(mut self, slot: impl IntoElement) -> Self {
        self.start_slot = Some(slot.into_any_element());
        self
    }

    /// Usually a close button.
    pub fn end_slot(mut self, slot: impl IntoElement) -> Self {
        self.end_slot = Some(slot.into_any_element());
        self
    }
}

impl InteractiveElement for Tab {
    fn interactivity(&mut self) -> &mut Interactivity {
        self.div.interactivity()
    }
}

impl StatefulInteractiveElement for Tab {}

impl Toggleable for Tab {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl ParentElement for Tab {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for Tab {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (text, bg) = if self.selected {
            (colors.text, colors.tab_active_background)
        } else {
            (colors.text_muted, colors.tab_inactive_background)
        };
        let slot = |size: f32, content: Option<AnyElement>| {
            div()
                .h_flex()
                .flex_none()
                .size(px(size))
                .justify_center()
                .children(content)
        };
        self.div
            .h(TAB_HEIGHT)
            .flex_none()
            .bg(bg)
            .border_color(colors.border)
            .cursor_pointer()
            .pl(px(1.))
            .map(|el| {
                if self.selected {
                    el.border_r_1().pb(px(1.))
                } else {
                    el.pr(px(1.)).border_b_1()
                }
            })
            .child(
                div()
                    .h_flex()
                    .h(TAB_HEIGHT - px(1.))
                    .px(px(4.))
                    .gap(rems_from_px(4.))
                    .text_color(text)
                    .child(slot(12., self.start_slot))
                    .children(self.children)
                    .child(slot(14., self.end_slot)),
            )
    }
}

/// A row of tabs with optional fixed controls at either end.
#[derive(IntoElement)]
pub struct TabBar {
    id: ElementId,
    start: SmallVec<[AnyElement; 2]>,
    tabs: SmallVec<[AnyElement; 2]>,
    end: SmallVec<[AnyElement; 2]>,
    scroll: Option<ScrollHandle>,
}

impl TabBar {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            start: SmallVec::new(),
            tabs: SmallVec::new(),
            end: SmallVec::new(),
            scroll: None,
        }
    }

    pub fn start_child(mut self, child: impl IntoElement) -> Self {
        self.start.push(child.into_any_element());
        self
    }

    pub fn end_child(mut self, child: impl IntoElement) -> Self {
        self.end.push(child.into_any_element());
        self
    }

    pub fn track_scroll(mut self, handle: &ScrollHandle) -> Self {
        self.scroll = Some(handle.clone());
        self
    }
}

impl ParentElement for TabBar {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.tabs.extend(elements);
    }
}

impl RenderOnce for TabBar {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let border = cx.theme().colors().border;
        let controls = |children: SmallVec<[AnyElement; 2]>| {
            div()
                .h_flex()
                .flex_none()
                .gap(rems_from_px(4.))
                .px(rems_from_px(6.))
                .border_b_1()
                .border_color(border)
                .children(children)
        };
        div()
            .id(self.id)
            .flex()
            .flex_none()
            .w_full()
            .h(TAB_HEIGHT)
            .bg(cx.theme().colors().tab_bar_background)
            .when(!self.start.is_empty(), |el| {
                el.child(controls(self.start).border_r_1())
            })
            .child(
                div()
                    .relative()
                    .flex_1()
                    .h_full()
                    .overflow_x_hidden()
                    .child(div().absolute().inset_0().border_b_1().border_color(border))
                    .child(
                        div()
                            .id("tabs")
                            .h_flex()
                            .flex_grow_1()
                            .overflow_x_scroll()
                            .when_some(self.scroll, |el, handle| el.track_scroll(&handle))
                            .children(self.tabs),
                    ),
            )
            .when(!self.end.is_empty(), |el| {
                el.child(controls(self.end).border_l_1())
            })
    }
}

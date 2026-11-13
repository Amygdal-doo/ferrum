//! [`ListItem`]: a selectable row for sidebars, pickers and menus.

use gpui::{
    AnyElement, AnyView, App, ClickEvent, ElementId, InteractiveElement, IntoElement, MouseButton,
    MouseDownEvent, ParentElement, Pixels, RenderOnce, SharedString, StatefulInteractiveElement,
    Styled, Window, div, prelude::FluentBuilder, px,
};
use smallvec::SmallVec;

use super::traits::{StyledExt, rems_from_px};
use super::{Disableable, Toggleable};
use crate::theme::ActiveTheme;

type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
type MouseHandler = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;
type TooltipBuilder = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// Vertical padding of a [`ListItem`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ListItemSpacing {
    #[default]
    Dense,
    Sparse,
}

#[derive(IntoElement)]
pub struct ListItem {
    id: ElementId,
    selected: bool,
    disabled: bool,
    rounded: bool,
    inset: bool,
    spacing: ListItemSpacing,
    indent_level: usize,
    indent_step: Pixels,
    group: Option<SharedString>,
    start_slot: Option<AnyElement>,
    end_slot: Option<AnyElement>,
    end_on_hover: bool,
    on_click: Option<ClickHandler>,
    on_secondary_mouse_down: Option<MouseHandler>,
    tooltip: Option<TooltipBuilder>,
    children: SmallVec<[AnyElement; 2]>,
}

impl ListItem {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            selected: false,
            disabled: false,
            rounded: false,
            inset: false,
            spacing: ListItemSpacing::Dense,
            indent_level: 0,
            indent_step: px(12.),
            group: None,
            start_slot: None,
            end_slot: None,
            end_on_hover: false,
            on_click: None,
            on_secondary_mouse_down: None,
            tooltip: None,
            children: SmallVec::new(),
        }
    }

    pub fn on_click(
        mut self,
        handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Box::new(handler));
        self
    }

    pub fn on_secondary_mouse_down(
        mut self,
        handler: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_secondary_mouse_down = Some(Box::new(handler));
        self
    }

    pub fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
    }

    /// Round the row's corners.
    pub fn rounded(mut self) -> Self {
        self.rounded = true;
        self
    }

    /// Pad the row inside its container (menu style).
    pub fn inset(mut self, inset: bool) -> Self {
        self.inset = inset;
        self
    }

    pub fn spacing(mut self, spacing: ListItemSpacing) -> Self {
        self.spacing = spacing;
        self
    }

    pub fn indent_level(mut self, level: usize) -> Self {
        self.indent_level = level;
        self
    }

    pub fn indent_step_size(mut self, step: Pixels) -> Self {
        self.indent_step = step;
        self
    }

    /// Hover group name for children using `visible_on_hover`.
    pub fn group_name(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }

    pub fn start_slot(mut self, slot: impl IntoElement) -> Self {
        self.start_slot = Some(slot.into_any_element());
        self
    }

    pub fn end_slot(mut self, slot: impl IntoElement) -> Self {
        self.end_slot = Some(slot.into_any_element());
        self
    }

    /// Show the end slot only while the row is hovered.
    pub fn end_hover_slot(mut self, slot: impl IntoElement) -> Self {
        self.end_slot = Some(slot.into_any_element());
        self.end_on_hover = true;
        self
    }
}

impl Toggleable for ListItem {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }
}

impl Disableable for ListItem {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl ParentElement for ListItem {
    fn extend(&mut self, elements: impl IntoIterator<Item = AnyElement>) {
        self.children.extend(elements);
    }
}

impl RenderOnce for ListItem {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let (hover, pressed, selected_bg) = (
            colors.ghost_element_hover,
            colors.ghost_element_active,
            colors.ghost_element_selected,
        );
        let interactive = !self.disabled;
        let indent = self.indent_step * self.indent_level as f32;
        let row_group = SharedString::from(format!("ferrum-list-item-{:?}", self.id));

        let row = div()
            .id(self.id)
            .group(row_group.clone())
            .h_flex()
            .w_full()
            .relative()
            .gap_1()
            .pr(px(6.))
            .pl(px(6.) + indent)
            .when(self.spacing == ListItemSpacing::Sparse, |el| el.py_1())
            .when(self.rounded || self.inset, |el| el.rounded_sm())
            .when(self.selected, |el| el.bg(selected_bg))
            .when(interactive, |el| {
                el.hover(move |s| s.bg(hover))
                    .active(move |s| s.bg(pressed))
            })
            .when_some(self.on_click.filter(|_| interactive), |el, handler| {
                el.cursor_pointer().on_click(handler)
            })
            .when_some(self.on_secondary_mouse_down, |el, handler| {
                el.on_mouse_down(MouseButton::Right, handler)
            })
            .when_some(self.tooltip, |el, tooltip| el.tooltip(tooltip))
            .child(
                div()
                    .h_flex()
                    .flex_grow_1()
                    .flex_shrink_0()
                    .flex_basis(gpui::relative(0.25))
                    .gap(rems_from_px(6.))
                    .overflow_hidden()
                    .children(self.start_slot)
                    .children(self.children),
            )
            .when_some(self.end_slot, |el, end| {
                el.justify_between().child(
                    div()
                        .h_flex()
                        .flex_shrink_1()
                        .overflow_hidden()
                        .when(self.end_on_hover, |slot| {
                            slot.invisible().group_hover(row_group, |s| s.visible())
                        })
                        .child(end),
                )
            });

        div()
            .w_full()
            .when_some(self.group, |el, group| el.group(group))
            .when(self.inset, |el| el.px(rems_from_px(4.)))
            .child(row)
    }
}

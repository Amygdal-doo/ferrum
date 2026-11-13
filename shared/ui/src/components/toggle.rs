//! [`Checkbox`].

use std::rc::Rc;

use gpui::{
    AnyView, App, ElementId, InteractiveElement, IntoElement, ParentElement, RenderOnce,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};

use super::traits::{StyledExt, rems_from_px};
use super::{
    Color, Disableable, Icon, IconName, IconSize, Label, LabelCommon, LabelSize, ToggleState,
};
use crate::theme::ActiveTheme;

type TooltipBuilder = Box<dyn Fn(&mut Window, &mut App) -> AnyView>;
type ToggleHandler = Rc<dyn Fn(&ToggleState, &mut Window, &mut App)>;

/// A checkbox with an optional label; clicking it (or the label) reports
/// the new state to [`Checkbox::on_click`].
#[derive(IntoElement)]
pub struct Checkbox {
    id: ElementId,
    state: ToggleState,
    disabled: bool,
    label: Option<SharedString>,
    label_size: LabelSize,
    on_click: Option<ToggleHandler>,
    tooltip: Option<TooltipBuilder>,
}

impl Checkbox {
    pub fn new(id: impl Into<ElementId>, state: ToggleState) -> Self {
        Self {
            id: id.into(),
            state,
            disabled: false,
            label: None,
            label_size: LabelSize::Default,
            on_click: None,
            tooltip: None,
        }
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = Some(label.into());
        self
    }

    pub fn label_size(mut self, size: LabelSize) -> Self {
        self.label_size = size;
        self
    }

    pub fn tooltip(mut self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self {
        self.tooltip = Some(Box::new(tooltip));
        self
    }

    /// Called with the state after the click.
    pub fn on_click(
        mut self,
        handler: impl Fn(&ToggleState, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.on_click = Some(Rc::new(handler));
        self
    }
}

impl Disableable for Checkbox {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for Checkbox {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let colors = cx.theme().colors();
        let mark_color = if self.disabled {
            Color::Disabled
        } else {
            Color::Selected
        };
        let mark = match self.state {
            ToggleState::Selected => Some(IconName::Check),
            ToggleState::Indeterminate => Some(IconName::Dash),
            ToggleState::Unselected => None,
        };
        let border = if self.disabled {
            colors.border_variant
        } else {
            colors.border
        };
        let group = SharedString::from(format!("ferrum-checkbox-{:?}", self.id));
        let next = self.state.inverse();
        let enabled = !self.disabled;

        let square = div()
            .size_4()
            .flex()
            .items_center()
            .justify_center()
            .rounded_xs()
            .border_1()
            .border_color(border)
            .bg(colors.ghost_element_background)
            .when(self.disabled, |el| {
                el.bg(colors.element_disabled.opacity(0.6))
            })
            .when(enabled, |el| {
                el.group_hover(group.clone(), move |s| s.border_color(border.alpha(0.7)))
            })
            .children(mark.map(|name| Icon::new(name).size(IconSize::Small).color(mark_color)));

        div()
            .id(self.id)
            .group(group)
            .h_flex()
            .gap(rems_from_px(6.))
            .child(
                div()
                    .flex_none()
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(square),
            )
            .when_some(self.label, |el, label| {
                el.child(
                    Label::new(label)
                        .size(self.label_size)
                        .color(if self.disabled {
                            Color::Disabled
                        } else {
                            Color::Default
                        }),
                )
            })
            .when(enabled, |el| el.cursor_pointer())
            .when_some(self.tooltip, |el, tooltip| el.tooltip(tooltip))
            .when_some(self.on_click.filter(|_| enabled), |el, handler| {
                el.on_click(move |_, window, cx| handler(&next, window, cx))
            })
    }
}

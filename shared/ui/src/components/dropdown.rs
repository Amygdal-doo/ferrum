//! [`DropdownMenu`]: a button showing the current choice that opens a
//! [`ContextMenu`] of alternatives.

use gpui::{
    AnyElement, App, ElementId, Entity, IntoElement, ParentElement, RenderOnce, SharedString,
    Styled, Window, div,
};

use super::button::ButtonBase;
use super::{
    ButtonStyle, Clickable, Color, ContextMenu, Disableable, Icon, IconName, IconSize, Label,
    LabelCommon, PopoverMenu, Toggleable,
};

/// The trigger's look.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DropdownStyle {
    #[default]
    Solid,
    Outlined,
    Subtle,
    Ghost,
}

impl From<DropdownStyle> for ButtonStyle {
    fn from(style: DropdownStyle) -> Self {
        match style {
            DropdownStyle::Solid => ButtonStyle::Filled,
            DropdownStyle::Outlined => ButtonStyle::Outlined,
            DropdownStyle::Subtle => ButtonStyle::Subtle,
            DropdownStyle::Ghost => ButtonStyle::Transparent,
        }
    }
}

/// The button inside a dropdown: label, then an up/down chevron.
#[derive(IntoElement)]
struct DropdownTrigger {
    base: ButtonBase,
    label: SharedString,
}

impl Clickable for DropdownTrigger {
    fn on_click(
        mut self,
        handler: impl Fn(&gpui::ClickEvent, &mut Window, &mut App) + 'static,
    ) -> Self {
        self.base.on_click = Some(std::rc::Rc::new(handler));
        self
    }

    fn cursor_style(mut self, cursor: gpui::CursorStyle) -> Self {
        self.base.cursor = cursor;
        self
    }
}

impl Toggleable for DropdownTrigger {
    fn toggle_state(mut self, selected: bool) -> Self {
        self.base.selected = selected;
        self
    }
}

impl RenderOnce for DropdownTrigger {
    fn render(self, _: &mut Window, cx: &mut App) -> impl IntoElement {
        let color = if self.base.disabled {
            Color::Disabled
        } else {
            Color::Default
        };
        let full = self.base.width.is_some();
        let label = Label::new(self.label).color(color).truncate();
        let label: AnyElement = if full {
            div().flex_1().min_w_0().child(label).into_any_element()
        } else {
            label.into_any_element()
        };
        let chevron = Icon::new(IconName::ChevronUpDown)
            .size(IconSize::XSmall)
            .color(Color::Muted);
        // The open state is shown by the menu itself, not by an accent label.
        let mut base = self.base;
        base.selected = false;
        base.render(vec![label, chevron.into_any_element()], cx)
            .when_full_width(full)
    }
}

/// Lays the trigger out edge to edge when it fills its container.
trait FullWidthExt {
    fn when_full_width(self, full: bool) -> Self;
}

impl<E: Styled> FullWidthExt for E {
    fn when_full_width(self, full: bool) -> Self {
        if full {
            self.justify_between().px_2()
        } else {
            self
        }
    }
}

#[derive(IntoElement)]
pub struct DropdownMenu {
    id: ElementId,
    label: SharedString,
    menu: Entity<ContextMenu>,
    style: DropdownStyle,
    full_width: bool,
    disabled: bool,
    aria_label: Option<SharedString>,
}

impl DropdownMenu {
    pub fn new(
        id: impl Into<ElementId>,
        label: impl Into<SharedString>,
        menu: Entity<ContextMenu>,
    ) -> Self {
        Self {
            id: id.into(),
            label: label.into(),
            menu,
            style: DropdownStyle::default(),
            full_width: false,
            disabled: false,
            aria_label: None,
        }
    }

    pub fn style(mut self, style: DropdownStyle) -> Self {
        self.style = style;
        self
    }

    pub fn full_width(mut self, full_width: bool) -> Self {
        self.full_width = full_width;
        self
    }

    /// Accessible name for screen readers.
    pub fn aria_label(mut self, label: impl Into<SharedString>) -> Self {
        self.aria_label = Some(label.into());
        self
    }
}

impl Disableable for DropdownMenu {
    fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }
}

impl RenderOnce for DropdownMenu {
    fn render(self, _: &mut Window, _: &mut App) -> impl IntoElement {
        let mut base = ButtonBase::new(self.id.clone());
        base.style = self.style.into();
        base.disabled = self.disabled;
        if self.full_width {
            base.width = Some(gpui::relative(1.));
        }
        base.aria_label = self.aria_label;
        let menu = self.menu;
        PopoverMenu::new(self.id)
            .full_width(self.full_width)
            .trigger(DropdownTrigger {
                base,
                label: self.label,
            })
            .menu(move |_, _| Some(menu.clone()))
    }
}

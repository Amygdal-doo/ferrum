//! Builder traits shared by the components, and flex/typography helpers
//! for any styled element.

use gpui::{
    AnyView, App, ClickEvent, CursorStyle, DefiniteLength, Div, ElementId, FocusHandle, Rems,
    SharedString, Styled, Window, div, rems,
};

use super::ButtonStyle;
use super::button::ButtonSize;
use crate::theme::{ActiveTheme, theme_settings};

/// Something that reacts to a left click.
pub trait Clickable {
    fn on_click(self, handler: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Self;

    fn cursor_style(self, cursor: CursorStyle) -> Self;
}

/// Something that can be greyed out and made inert.
pub trait Disableable {
    fn disabled(self, disabled: bool) -> Self;
}

/// Something with an on/off (selected) state.
pub trait Toggleable {
    fn toggle_state(self, selected: bool) -> Self;
}

/// Something with a settable width.
pub trait FixedWidth {
    fn width(self, width: impl Into<DefiniteLength>) -> Self;

    fn full_width(self) -> Self;
}

/// A button that may look different while selected.
pub trait SelectableButton {
    fn selected_style(self, style: ButtonStyle) -> Self;
}

/// Builder methods every button kind shares.
pub trait ButtonCommon: Clickable + Disableable {
    fn id(&self) -> &ElementId;

    fn style(self, style: ButtonStyle) -> Self;

    fn size(self, size: ButtonSize) -> Self;

    fn tooltip(self, tooltip: impl Fn(&mut Window, &mut App) -> AnyView + 'static) -> Self;

    fn track_focus(self, focus: &FocusHandle) -> Self;
}

/// Checkbox state; `Indeterminate` shows a dash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ToggleState {
    #[default]
    Unselected,
    Indeterminate,
    Selected,
}

impl ToggleState {
    pub fn selected(self) -> bool {
        self == ToggleState::Selected
    }

    /// Selected becomes unselected; anything else becomes selected.
    pub fn inverse(self) -> Self {
        if self.selected() {
            ToggleState::Unselected
        } else {
            ToggleState::Selected
        }
    }
}

impl From<bool> for ToggleState {
    fn from(selected: bool) -> Self {
        if selected {
            ToggleState::Selected
        } else {
            ToggleState::Unselected
        }
    }
}

/// Hide an element until the pointer is over the group named `group`.
pub trait VisibleOnHover {
    fn visible_on_hover(self, group: impl Into<SharedString>) -> Self;
}

impl<E: Styled + gpui::InteractiveElement> VisibleOnHover for E {
    fn visible_on_hover(self, group: impl Into<SharedString>) -> Self {
        self.invisible().group_hover(group, |style| style.visible())
    }
}

/// A horizontal flex row with centred items.
pub fn h_flex() -> Div {
    div().h_flex()
}

/// A vertical flex column.
pub fn v_flex() -> Div {
    div().v_flex()
}

/// Layout and surface helpers for any styled element.
pub trait StyledExt: Styled + Sized {
    fn h_flex(self) -> Self {
        self.flex().flex_row().items_center()
    }

    fn v_flex(self) -> Self {
        self.flex().flex_col()
    }

    /// Raised surface for popovers and menus.
    fn elevation_2(self, cx: &App) -> Self {
        let colors = cx.theme().colors();
        self.bg(colors.elevated_surface_background)
            .rounded_lg()
            .border_1()
            .border_color(colors.border_variant)
            .shadow_md()
    }

    /// Topmost surface for dialogs.
    fn elevation_3(self, cx: &App) -> Self {
        let colors = cx.theme().colors();
        self.bg(colors.elevated_surface_background)
            .rounded_lg()
            .border_1()
            .border_color(colors.border_variant)
            .shadow_lg()
    }
}

impl<E: Styled> StyledExt for E {}

/// Fixed UI text sizes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TextSize {
    Large,
    #[default]
    Default,
    Small,
    XSmall,
}

impl TextSize {
    pub fn rems(self) -> Rems {
        let px = match self {
            TextSize::Large => 16.,
            TextSize::Default => 14.,
            TextSize::Small => 12.,
            TextSize::XSmall => 10.,
        };
        rems_from_px(px)
    }
}

/// `px` as rems at the default 16px root size.
pub fn rems_from_px(px: f32) -> Rems {
    rems(px / 16.)
}

/// Font family and size helpers that follow [`crate::theme::ThemeSettings`].
pub trait StyledTypography: Styled + Sized {
    fn font_ui(self, cx: &App) -> Self {
        self.font_family(theme_settings(cx).ui_font.family.clone())
    }

    fn font_buffer(self, cx: &App) -> Self {
        let font = theme_settings(cx).buffer_font.clone();
        self.font_family(font.family)
    }

    fn text_ui_size(self, size: TextSize, _cx: &App) -> Self {
        self.text_size(size.rems())
    }

    fn text_ui_lg(self, cx: &App) -> Self {
        self.text_ui_size(TextSize::Large, cx)
    }

    fn text_ui(self, cx: &App) -> Self {
        self.text_ui_size(TextSize::Default, cx)
    }

    fn text_ui_sm(self, cx: &App) -> Self {
        self.text_ui_size(TextSize::Small, cx)
    }

    fn text_ui_xs(self, cx: &App) -> Self {
        self.text_ui_size(TextSize::XSmall, cx)
    }

    /// The code font at the code size.
    fn text_buffer(self, cx: &App) -> Self {
        let settings = theme_settings(cx);
        let size = settings.buffer_font_size;
        self.font_buffer(cx).text_size(size)
    }
}

impl<E: Styled> StyledTypography for E {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn toggle_state_from_bool_and_inverse() {
        assert_eq!(ToggleState::from(true), ToggleState::Selected);
        assert_eq!(ToggleState::Selected.inverse(), ToggleState::Unselected);
        assert_eq!(ToggleState::Indeterminate.inverse(), ToggleState::Selected);
    }

    #[test]
    fn text_sizes_are_pixel_steps() {
        assert_eq!(TextSize::Small.rems(), rems(0.75));
        assert_eq!(TextSize::Large.rems(), rems(1.));
    }
}

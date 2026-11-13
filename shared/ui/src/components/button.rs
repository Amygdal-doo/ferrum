//! Button styles and sizes, and the clickable frame [`Button`] and
//! [`IconButton`] are drawn in.

use std::rc::Rc;

use gpui::{
    AnyElement, AnyView, App, ClickEvent, CursorStyle, DefiniteLength, Div, ElementId, FocusHandle,
    Hsla, InteractiveElement, MouseButton, ParentElement, Rems, Stateful,
    StatefulInteractiveElement, Styled, Window, prelude::FluentBuilder, transparent_black,
};

use super::TintColor;
use super::traits::{StyledExt, StyledTypography, rems_from_px};
use crate::theme::ActiveTheme;

/// How a button is painted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum ButtonStyle {
    /// Solid element background.
    Filled,
    /// Solid status colour.
    Tinted(TintColor),
    /// Element background with a border.
    Outlined,
    /// Transparent with a border.
    OutlinedGhost,
    /// Transparent until hovered.
    #[default]
    Subtle,
    /// Always transparent; hover only mutes the content.
    Transparent,
}

/// Button heights (32 / 28 / 22 / 18 / 16 px).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonSize {
    Large,
    Medium,
    #[default]
    Default,
    Compact,
    None,
}

impl ButtonSize {
    pub fn rems(self) -> Rems {
        rems_from_px(match self {
            ButtonSize::Large => 32.,
            ButtonSize::Medium => 28.,
            ButtonSize::Default => 22.,
            ButtonSize::Compact => 18.,
            ButtonSize::None => 16.,
        })
    }

    fn padding_x(self) -> Rems {
        rems_from_px(match self {
            ButtonSize::Large | ButtonSize::Medium => 8.,
            ButtonSize::Default | ButtonSize::Compact => 4.,
            ButtonSize::None => 1.,
        })
    }
}

/// Resting, hovered and pressed fills plus the border of a style.
struct Fills {
    rest: Hsla,
    hover: Hsla,
    pressed: Hsla,
    border: Option<(Hsla, Hsla)>,
}

impl ButtonStyle {
    fn fills(self, cx: &App) -> Fills {
        let theme = cx.theme();
        let c = theme.colors();
        let clear = transparent_black();
        match self {
            ButtonStyle::Filled => Fills {
                rest: c.element_background,
                hover: c.element_background.opacity(0.5),
                pressed: c.element_active,
                border: None,
            },
            ButtonStyle::Tinted(tint) => {
                let (bg, border) = tint.fill(cx);
                Fills {
                    rest: bg,
                    hover: theme.darken(bg, 0.05, 0.2),
                    pressed: bg,
                    border: Some((border, border)),
                }
            }
            ButtonStyle::Outlined => Fills {
                rest: c.element_background,
                hover: c.ghost_element_hover,
                pressed: c.element_active,
                border: Some((c.border_variant, c.border)),
            },
            ButtonStyle::OutlinedGhost => Fills {
                rest: clear,
                hover: c.ghost_element_hover,
                pressed: clear,
                border: Some((c.border_variant, c.border)),
            },
            ButtonStyle::Subtle => Fills {
                rest: c.ghost_element_background,
                hover: c.ghost_element_hover,
                pressed: c.ghost_element_active,
                border: None,
            },
            ButtonStyle::Transparent => Fills {
                rest: clear,
                hover: clear,
                pressed: clear,
                border: None,
            },
        }
    }
}

pub(crate) type ClickHandler = Rc<dyn Fn(&ClickEvent, &mut Window, &mut App)>;
pub(crate) type TooltipBuilder = Rc<dyn Fn(&mut Window, &mut App) -> AnyView>;

/// State shared by every button kind.
pub(crate) struct ButtonBase {
    pub id: ElementId,
    pub style: ButtonStyle,
    pub selected_style: Option<ButtonStyle>,
    pub size: ButtonSize,
    pub disabled: bool,
    pub selected: bool,
    pub width: Option<DefiniteLength>,
    pub cursor: CursorStyle,
    pub tooltip: Option<TooltipBuilder>,
    pub on_click: Option<ClickHandler>,
    pub focus: Option<FocusHandle>,
    pub hover_group: Option<gpui::SharedString>,
    pub aria_label: Option<gpui::SharedString>,
}

impl ButtonBase {
    pub fn new(id: impl Into<ElementId>) -> Self {
        Self {
            id: id.into(),
            style: ButtonStyle::default(),
            selected_style: None,
            size: ButtonSize::default(),
            disabled: false,
            selected: false,
            width: None,
            cursor: CursorStyle::PointingHand,
            tooltip: None,
            on_click: None,
            focus: None,
            hover_group: None,
            aria_label: None,
        }
    }

    /// The frame with `children` laid out in a row.
    pub fn render(self, children: Vec<AnyElement>, cx: &App) -> Stateful<Div> {
        let style = match self.selected_style {
            Some(selected) if self.selected => selected,
            _ => self.style,
        };
        let fills = style.fills(cx);
        let height = self.size.rems();
        let focus_border = cx.theme().colors().border_focused;
        let clickable = !self.disabled;

        gpui::div()
            .id(self.id)
            .h_flex()
            .flex_none()
            .font_ui(cx)
            .h(height)
            .gap(rems_from_px(4.))
            .rounded_sm()
            .bg(fills.rest)
            .px(self.size.padding_x())
            .when_some(self.width, |el, width| el.w(width).justify_center())
            .when_some(fills.border, |el, (rest, _)| {
                el.border_1()
                    .border_color(rest)
                    .focus_visible(move |s| s.border_color(focus_border))
            })
            .when_some(self.focus, |el, focus| el.track_focus(&focus))
            .when(self.disabled, |el| el.cursor_not_allowed())
            .when(clickable, |el| {
                let (hover, pressed) = (fills.hover, fills.pressed);
                let hover_border = fills.border.map(|(_, border)| border);
                el.cursor(self.cursor)
                    .hover(move |s| {
                        let s = s.bg(hover);
                        match hover_border {
                            Some(border) => s.border_color(border),
                            None => s,
                        }
                    })
                    .active(move |s| s.bg(pressed))
            })
            .when_some(self.on_click.filter(|_| clickable), |el, handler| {
                el.on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                    .on_click(move |event, window, cx| {
                        cx.stop_propagation();
                        handler(event, window, cx);
                    })
            })
            .when_some(self.tooltip, |el, tooltip| {
                el.tooltip(move |window, cx| tooltip(window, cx))
            })
            .when_some(self.aria_label, |el, label| el.aria_label(label))
            .when_some(self.hover_group, |el, group| {
                el.invisible().group_hover(group, |s| s.visible())
            })
            .children(children)
    }
}

impl From<ButtonStyle> for super::Color {
    /// The content colour a style implies (its tint, else the default).
    fn from(style: ButtonStyle) -> Self {
        match style {
            ButtonStyle::Tinted(tint) => tint.into(),
            _ => super::Color::Default,
        }
    }
}

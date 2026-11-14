//! Settings › Interface: colour scheme, toolbar density, text size, Dock
//! icon and the onboarding walkthrough.

use gpui::{AnyElement, App, Hsla, IntoElement, ParentElement, Styled, Window, px};

use super::widgets::{
    ChoiceCard, Row, Section, Segmented, Switch, scheme_preview, style_preview, toolbar_preview,
};
use super::{SettingsPage, ShowOnboarding, tint};
use crate::appearance::{self, ColorScheme, InterfaceStyle, TextSize, ToolbarStyle};
use crate::components::{Button, Clickable, IconName, h_flex};

pub(super) struct InterfacePage;

/// The colour scheme and toolbar pickers, shared with onboarding.
pub(crate) fn look_pickers(cx: &App) -> impl IntoElement {
    let prefs = appearance::current(cx);
    let schemes = [
        (ColorScheme::Auto, "Auto"),
        (ColorScheme::Light, "Light"),
        (ColorScheme::Dark, "Dark"),
    ];
    let toolbars = [
        (ToolbarStyle::Comfortable, "Comfortable"),
        (ToolbarStyle::Compact, "Compact"),
    ];
    h_flex()
        .py_3()
        .gap_3()
        .flex_wrap()
        .children(schemes.map(|(scheme, label)| {
            ChoiceCard::new(
                ("scheme", scheme as usize),
                label,
                scheme_preview(scheme, cx),
            )
            .width(px(112.))
            .selected(prefs.scheme == scheme)
            .on_click(move |_, _, cx| appearance::update(cx, |p| p.scheme = scheme))
        }))
        .child(gpui::div().w(px(16.)))
        .children(toolbars.map(|(style, label)| {
            ChoiceCard::new(
                ("toolbar", style as usize),
                label,
                toolbar_preview(style, cx),
            )
            .width(px(112.))
            .selected(prefs.toolbar == style)
            .on_click(move |_, _, cx| appearance::update(cx, |p| p.toolbar = style))
        }))
}

/// The Solid / Gradient / Glass cards, shared with onboarding.
pub(crate) fn style_pickers(cx: &App) -> impl IntoElement {
    let current = appearance::current(cx).style;
    h_flex()
        .py_3()
        .gap_3()
        .flex_wrap()
        .children(InterfaceStyle::ALL.map(|style| {
            ChoiceCard::new(
                ("style", style as usize),
                style.label(),
                style_preview(style, cx),
            )
            .width(px(112.))
            .selected(current == style)
            .on_click(move |_, _, cx| appearance::update(cx, |p| p.style = style))
        }))
}

impl SettingsPage for InterfacePage {
    fn id(&self) -> &'static str {
        "interface"
    }
    fn title(&self) -> &'static str {
        "Interface"
    }
    fn icon(&self) -> IconName {
        IconName::Blocks
    }
    fn tint(&self) -> Hsla {
        tint(0x8b8b90)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &[
            "appearance",
            "dark",
            "light",
            "scheme",
            "toolbar",
            "text size",
            "dock",
            "onboarding",
        ]
    }

    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let prefs = appearance::current(cx);
        let sizes = [TextSize::Small, TextSize::Default, TextSize::Large];
        let size_index = sizes
            .iter()
            .position(|s| *s == prefs.text_size)
            .unwrap_or(1);
        let in_dock = !crate::dock::hidden_by_preference();
        let tinted = usize::from(prefs.title_bar == appearance::TitleBarStyle::Tinted);
        let info = crate::app_info::app_info(cx);
        let dock_info = info.clone();
        let extra_rows: Vec<AnyElement> =
            info.window_rows.iter().map(|row| row(window, cx)).collect();
        gpui::div()
            .flex()
            .flex_col()
            .gap_6()
            .child(Section::new("Look").row(look_pickers(cx)).footnote(format!(
                "Auto follows the system. The colour scheme is {}'s own; \
                         toolbar, style and text size apply to every Ferrum app.",
                info.name
            )))
            .child(
                Section::new("Interface style")
                    .row(style_pickers(cx))
                    .footnote(
                        "Gradient puts translucent panes on a colour wash from the theme; Glass \
                         shows the desktop, blurred, behind them.",
                    ),
            )
            .child(
                Section::new("Window")
                    .row(
                        Row::new("title-bar", "Title bar")
                            .detail(
                                "Transparent: the top row sits on the window background. \
                                 Tinted: a shaded strip like a classic title bar.",
                            )
                            .control(
                                Segmented::new("title-bar-segments", tinted)
                                    .item("Transparent")
                                    .item("Tinted")
                                    .on_select(|i, _, cx| {
                                        appearance::update(cx, |p| {
                                            p.title_bar = if i == 1 {
                                                appearance::TitleBarStyle::Tinted
                                            } else {
                                                appearance::TitleBarStyle::Transparent
                                            }
                                        });
                                    }),
                            ),
                    )
                    .row(
                        Row::new("text-size", "Text size")
                            .detail("Size of labels, lists and menus in every window.")
                            .control(
                                Segmented::new("text-size-segments", size_index)
                                    .item("Small")
                                    .item("Default")
                                    .item("Large")
                                    .on_select(move |i, _, cx| {
                                        appearance::update(cx, |p| p.text_size = sizes[i])
                                    }),
                            ),
                    )
                    .row(
                        Row::new("dock-icon", "Show in Dock")
                            .detail("Off: the app runs without a Dock icon or ⌘-Tab entry.")
                            .control(Switch::new("dock-icon-switch", in_dock).on_change(
                                move |on, _, cx| {
                                    if let Some(on_dock) = &dock_info.on_dock {
                                        on_dock(on, cx);
                                    } else {
                                        crate::dock::set_hidden(dock_info.binary, !on);
                                        crate::dock::set_dock_icon_visible(on);
                                    }
                                    cx.refresh_windows();
                                },
                            )),
                    )
                    .rows(extra_rows),
            )
            .child(
                Section::new("Getting started").row(
                    Row::new("onboarding", "Onboarding")
                        .detail("Walk through appearance, permissions and migration again.")
                        .control(Button::new("show-onboarding", "Open…").on_click(
                            |_, window, cx| window.dispatch_action(Box::new(ShowOnboarding), cx),
                        )),
                ),
            )
            .into_any_element()
    }
}

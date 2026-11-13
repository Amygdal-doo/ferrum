//! Settings › Interface: colour scheme, toolbar density, text size, Dock
//! icon and the onboarding walkthrough.

use gpui::{AnyElement, App, Hsla, IntoElement, ParentElement, Styled, Window, px};

use super::widgets::{
    ChoiceCard, Row, Section, Segmented, Switch, scheme_preview, toolbar_preview,
};
use super::{SettingsPage, ShowOnboarding, tint};
use crate::appearance::{self, ColorScheme, TextSize, ToolbarStyle};
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
            ChoiceCard::new(("scheme", scheme as usize), label, scheme_preview(scheme))
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
            .selected(prefs.toolbar == style)
            .on_click(move |_, _, cx| appearance::update(cx, |p| p.toolbar = style))
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

    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let prefs = appearance::current(cx);
        let sizes = [TextSize::Small, TextSize::Default, TextSize::Large];
        let size_index = sizes
            .iter()
            .position(|s| *s == prefs.text_size)
            .unwrap_or(1);
        let in_dock = !crate::dock::hidden_by_preference();
        let info = crate::app_info::app_info(cx);
        gpui::div()
            .flex()
            .flex_col()
            .gap_6()
            .child(
                Section::new("Look")
                    .row(look_pickers(cx))
                    .footnote("Auto follows the system. Every Ferrum app uses the same look."),
            )
            .child(
                Section::new("Window")
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
                                    crate::dock::set_hidden(info.binary, !on);
                                    crate::dock::set_dock_icon_visible(on);
                                    cx.refresh_windows();
                                },
                            )),
                    ),
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

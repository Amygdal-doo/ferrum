//! Settings › About: the app's name, version and links (also the target of
//! the app menu's "About …").

use gpui::{AnyElement, App, Hsla, IntoElement, ParentElement, Styled, Window, px};

use super::widgets::{Row, Section, icon_tile, page_title};
use super::{SettingsPage, tint};
use crate::app_info::app_info;
use crate::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, Color, IconName, Label, LabelCommon, LabelSize,
    h_flex, v_flex,
};

pub(super) struct AboutPage;

/// The version this build was released as (`FERRUM_VERSION` at build time).
pub fn version() -> &'static str {
    option_env!("FERRUM_VERSION").unwrap_or(env!("CARGO_PKG_VERSION"))
}

impl SettingsPage for AboutPage {
    fn id(&self) -> &'static str {
        "about"
    }
    fn title(&self) -> &'static str {
        "About"
    }
    fn icon(&self) -> IconName {
        IconName::Info
    }
    fn tint(&self) -> Hsla {
        tint(0x8b8b90)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["version", "update", "help", "website"]
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let info = app_info(cx);
        let binary = info.binary;
        v_flex()
            .gap_6()
            .child(
                h_flex()
                    .gap_4()
                    .child(icon_tile(info.icon, gpui::rgb(info.tint).into(), px(56.)))
                    .child(
                        v_flex()
                            .child(page_title(info.name))
                            .child(Label::new(format!("Version {}", version())).color(Color::Muted))
                            .when(!info.tagline.is_empty(), |this| {
                                this.child(
                                    Label::new(info.tagline)
                                        .size(LabelSize::Small)
                                        .color(Color::Muted),
                                )
                            }),
                    ),
            )
            .child(
                Section::new("Updates").row(
                    Row::new("check-updates", "Check for updates")
                        .detail("FerrumLauncher installs and updates every Ferrum app.")
                        .control(
                            Button::new("open-launcher", "Open FerrumLauncher")
                                .style(ButtonStyle::Subtle)
                                .on_click(|_, _, _| crate::menus::open_launcher()),
                        ),
                ),
            )
            .child(
                Section::new("Help")
                    .row(
                        Row::new("help-page", format!("{} on the web", info.name)).control(
                            Button::new("open-help", "Open")
                                .style(ButtonStyle::Subtle)
                                .on_click(move |_, _, cx| {
                                    cx.open_url(&crate::menus::help_url(binary))
                                }),
                        ),
                    )
                    .row(
                        Row::new("release-notes", "Release notes").control(
                            Button::new("open-changelog", "Open")
                                .style(ButtonStyle::Subtle)
                                .on_click(|_, _, cx| {
                                    cx.open_url("https://tryferrum.dev/changelog")
                                }),
                        ),
                    ),
            )
            .into_any_element()
    }
}

use gpui::prelude::FluentBuilder as _;

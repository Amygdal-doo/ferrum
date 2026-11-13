//! Settings › Themes: the built-in themes for light and dark mode.

use gpui::{
    AnyElement, App, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window,
};

use super::widgets::{ChoiceCard, Section, theme_preview};
use super::{SettingsPage, tint};
use crate::appearance;
use crate::components::{Color, IconName, Label, LabelCommon, LabelSize, h_flex};
use crate::theme::Appearance;
use crate::theme::presets::PRESETS;

pub(super) struct ThemesPage;

/// The theme cards for one appearance, shared with onboarding.
pub(crate) fn theme_row(appearance_: Appearance, cx: &App) -> impl IntoElement {
    let prefs = appearance::current(cx);
    let chosen = prefs.preset(appearance_).id;
    h_flex()
        .id(match appearance_ {
            Appearance::Dark => "dark-themes",
            Appearance::Light => "light-themes",
        })
        .w_full()
        .py_3()
        .gap_3()
        .overflow_x_scroll()
        .children(
            PRESETS
                .iter()
                .filter(|p| p.appearance == appearance_)
                .map(|preset| {
                    let id = preset.id;
                    ChoiceCard::new(id, preset.name, theme_preview(preset))
                        .width(gpui::px(132.))
                        .selected(id == chosen)
                        .on_click(move |_, _, cx| {
                            appearance::update(cx, |p| p.choose_theme(id, appearance_))
                        })
                }),
        )
}

impl SettingsPage for ThemesPage {
    fn id(&self) -> &'static str {
        "themes"
    }
    fn title(&self) -> &'static str {
        "Themes"
    }
    fn icon(&self) -> IconName {
        IconName::Sparkle
    }
    fn tint(&self) -> Hsla {
        tint(0xc792ea)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["colors", "colours", "palette", "dark mode", "light mode"]
    }

    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        let active = appearance::effective(cx);
        let heading = |name: &'static str, a: Appearance| {
            let mut section = Section::new(name).row(theme_row(a, cx));
            if a == active {
                section = section.accessory(
                    Label::new("Active")
                        .size(LabelSize::Small)
                        .color(Color::Accent),
                );
            }
            section
        };
        gpui::div()
            .flex()
            .flex_col()
            .gap_6()
            .child(heading("Light Mode", Appearance::Light))
            .child(heading("Dark Mode", Appearance::Dark).footnote(
                "With the Auto colour scheme the system decides which of the two is used. \
                 Ferrum Editor also lists its own and imported themes under Theme: Select.",
            ))
            .into_any_element()
    }
}

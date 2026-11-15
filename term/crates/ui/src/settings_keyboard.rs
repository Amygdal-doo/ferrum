//! Settings › Keyboard (FerrumTerm): what the Option key does in a
//! terminal. The same section is on the Terminal page that FerrumIDE
//! and FerrumEngineer show.

use ferrum_ui::components::IconName;
use ferrum_ui::gpui::{self, AnyElement, App, Hsla, IntoElement, Window, div, prelude::*, px};
use ferrum_ui::settings::SettingsPage;
use ferrum_ui::settings::widgets::{Row, Section};
use ferrum_ui::theme::ActiveTheme;

use crate::settings::{self, OptionKey};

/// A radio mark: a ring, filled in the middle when selected.
fn radio(selected: bool, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors();
    div()
        .flex_none()
        .size(px(16.))
        .rounded_full()
        .border_1()
        .border_color(if selected {
            colors.text_accent
        } else {
            colors.border
        })
        .flex()
        .items_center()
        .justify_center()
        .when(selected, |ring| {
            ring.child(div().size(px(8.)).rounded_full().bg(colors.text_accent))
        })
}

/// "Option key": one radio row per choice.
pub(crate) fn option_key_section(cx: &App) -> Section {
    let current = settings::get().option_key();
    let mut section = Section::new("Option key")
        .detail("Use Option for terminal shortcuts or to type accents and special characters.");
    for (i, choice) in OptionKey::ALL.into_iter().enumerate() {
        section = section.row(
            Row::new(("option-key", i), choice.label())
                .detail(choice.detail())
                .icon(radio(choice == current, cx))
                .on_click(move |_, _, cx| {
                    settings::update(|s| {
                        s.option = Some(choice);
                        s.option_as_meta = None;
                    });
                    cx.refresh_windows();
                }),
        );
    }
    section.footnote(
        "Applies to every terminal on this Mac, including the panes in FerrumIDE and \
         FerrumEngineer. ⌘ shortcuts are not affected.",
    )
}

/// Settings › Keyboard.
#[derive(Default)]
pub struct KeyboardPage;

impl SettingsPage for KeyboardPage {
    fn id(&self) -> &'static str {
        "keyboard"
    }
    fn title(&self) -> &'static str {
        "Keyboard"
    }
    fn icon(&self) -> IconName {
        IconName::Command
    }
    fn tint(&self) -> Hsla {
        gpui::rgb(0x8e8e93).into()
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["option", "alt", "meta", "accents", "keys"]
    }
    fn render(&self, _: &mut Window, cx: &mut App) -> AnyElement {
        div()
            .w_full()
            .flex()
            .flex_col()
            .gap_6()
            .child(option_key_section(cx))
            .into_any_element()
    }
}

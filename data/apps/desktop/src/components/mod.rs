pub mod compare;
pub mod connection;
pub mod diagram;
pub mod dialogs;
pub mod explorer;
pub mod history;
pub mod key_view;
pub mod plan;
pub mod result_grid;
pub mod shell;
pub mod sql_editor;
pub mod table_view;
pub mod transfer;

use ferrum_ui::gpui::{App, Hsla, div, prelude::*, px, rgb};
use ferrum_ui::theme::ActiveTheme;
use ferrumdata_domain::EnvironmentTag;

/// Colour of the environment dot and badges.
#[must_use]
pub fn env_color(env: EnvironmentTag, cx: &App) -> Hsla {
    match env {
        EnvironmentTag::Local => cx.theme().colors().text_muted,
        EnvironmentTag::Development => rgb(0x0ea5e9).into(),
        EnvironmentTag::Staging => rgb(0xf59e0b).into(),
        EnvironmentTag::Production => rgb(0xdc2626).into(),
    }
}

/// Small round environment indicator.
pub fn env_dot(env: EnvironmentTag, size: f32, cx: &App) -> impl IntoElement {
    div()
        .flex_none()
        .size(px(size))
        .rounded_full()
        .bg(env_color(env, cx))
}

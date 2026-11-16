//! Reusable editing widgets.

mod auth_editor;
mod auth_signing;
mod kv_table;
mod secret_input;
mod vars_table;

pub use auth_editor::{AuthTarget, AuthView};
pub use kv_table::RowEditor;

use ferrum_ui::components::{ActiveTheme as _, Color, Label, LabelCommon as _, LabelSize};
use gpui::{App, Hsla, IntoElement, div, prelude::*};

/// Tooltip listing unresolved variables (empty when none).
#[must_use]
pub fn unresolved_title(names: &[String]) -> String {
    if names.is_empty() {
        String::new()
    } else {
        format!("Unresolved: {}", names.join(", "))
    }
}

/// Column header cell.
pub fn header_cell(text: &'static str) -> impl IntoElement {
    div()
        .px_2()
        .py_1()
        .child(Label::new(text).size(LabelSize::XSmall).color(Color::Muted))
}

/// Row divider colour.
pub fn row_border(cx: &App) -> Hsla {
    cx.theme().colors().border_variant
}

/// Labelled form row: fixed-width label, then the control.
pub fn form_row(label: &'static str, control: impl IntoElement) -> impl IntoElement {
    ferrum_ui::components::h_flex()
        .gap_3()
        .child(
            div()
                .w_24()
                .flex_none()
                .child(Label::new(label).size(LabelSize::Small).color(Color::Muted)),
        )
        .child(div().flex_1().min_w_0().child(control))
}

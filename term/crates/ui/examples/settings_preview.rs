//! FerrumTerm's onboarding and Settings without starting a terminal
//! server: `FERRUM_OPEN_SETTINGS=terminal cargo run -p ferrumterm-ui --example settings_preview`.

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::{AppContext as _, Render, div};
use ferrum_ui::prelude::*;

struct Blank;

impl Render for Blank {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .bg(ferrum_ui::theme::ActiveTheme::theme(&**cx)
                .colors()
                .terminal_background)
    }
}

fn main() {
    ferrum_ui::run(
        AppConfig::new("FerrumTerm")
            .size(1100., 700.)
            .app(ferrumterm_ui::app_info()),
        |_, cx| {
            ferrumterm_ui::init(cx);
            cx.new(|_| Blank)
        },
    );
}

//! A terminal pane as FerrumIDE embeds it (compact, private-server
//! fallback): `cargo run -p ferrumterm-ui --example embedded_preview`.

use ferrum_ui::AppConfig;
use ferrumterm_ui::{TermView, TermViewOptions};

fn main() {
    ferrum_ui::run(
        AppConfig::new("Embedded terminal").size(900., 360.).app(
            ferrum_ui::AppInfo::new("Embedded terminal", "embedded-preview").without_onboarding(),
        ),
        |window, cx| {
            ferrumterm_ui::init(cx);
            TermView::new(
                TermViewOptions::embedded().session_name("editor:preview"),
                window,
                cx,
            )
        },
    );
}

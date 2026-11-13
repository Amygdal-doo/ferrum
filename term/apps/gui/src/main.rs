//! FerrumTerm: a GPUI terminal window on top of the persistent FerrumTerm
//! server. Tabs are the session's windows, splits are its blocks. Close the
//! window any time — agents keep running; reopen (or `fterm attach` from
//! anywhere, even over SSH) to pick up where you left off.
//!
//! The window's content is `ferrumterm_ui::TermView`, the same view Ferrum
//! Editor and FerrumAgentic embed as a pane.

use ferrum_ui::AppConfig;
use ferrumterm_ui::{TermView, TermViewOptions};

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    ferrum_ui::run(
        AppConfig::new("FerrumTerm")
            .size(1100., 700.)
            .min_size(420., 260.),
        |window, cx| {
            ferrumterm_ui::init(cx);
            TermView::new(TermViewOptions::standalone(), window, cx)
        },
    );
}

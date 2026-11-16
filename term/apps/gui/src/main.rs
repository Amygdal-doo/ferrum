//! FerrumTerm: a GPUI terminal window on top of the persistent FerrumTerm
//! server. Tabs are the session's windows, splits are its blocks. Close the
//! window any time — agents keep running; reopen (or `fterm attach` from
//! anywhere, even over SSH) to pick up where you left off.
//!
//! The window's content is `ferrumterm_ui::TermView`, the same view
//! FerrumIDE and FerrumEngineer embed as a pane.

use ferrum_ui::AppConfig;
// The view posts agent notifications itself (toast, system, sound;
// clicking the toast shows the terminal), see `TermViewOptions::standalone`.
use ferrumterm_ui::{TermView, TermViewOptions};

fn main() {
    // Git asks for credentials by running this executable as its askpass
    // helper (the FerrumGIT pane).
    if let Some(code) = ferrumgit_ui::askpass_main() {
        std::process::exit(code);
    }
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();

    ferrum_ui::run(
        AppConfig::new("FerrumTerm")
            .unified_titlebar()
            .size(1100., 700.)
            .min_size(420., 260.)
            .app(ferrumterm_ui::app_info()),
        |window, cx| {
            ferrum_account_ui::init(ferrum_account_ui::AccountApp::Term, cx);
            ferrumterm_ui::init(cx);
            ferrumgit_ui::init(cx);
            TermView::new(TermViewOptions::standalone(), window, cx)
        },
    );
}

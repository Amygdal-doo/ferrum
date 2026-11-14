//! FerrumTerm: a GPUI terminal window on top of the persistent FerrumTerm
//! server. Tabs are the session's windows, splits are its blocks. Close the
//! window any time — agents keep running; reopen (or `fterm attach` from
//! anywhere, even over SSH) to pick up where you left off.
//!
//! The window's content is `ferrumterm_ui::TermView`, the same view Ferrum
//! Editor and FerrumVibe embed as a pane.

use ferrum_ui::AppConfig;
use ferrum_ui::notify;
use ferrumterm_ui::{ProgramState, TermView, TermViewEvent, TermViewOptions};

/// In-app toast, badge and sound for an agent's status; the FerrumTerm
/// server posts the system notification itself.
fn on_view_event(event: &TermViewEvent, cx: &mut ferrum_ui::gpui::App) {
    let TermViewEvent::AgentStatus {
        title,
        status: Some(status),
        ..
    } = event
    else {
        return;
    };
    let (id, headline) = match status.state {
        ProgramState::Working => ("working", "is working"),
        ProgramState::Blocked => ("blocked", "is waiting for you"),
        ProgramState::Error => ("error", "failed"),
        ProgramState::Done => ("done", "finished"),
        ProgramState::Idle => return,
    };
    let body = status
        .message
        .clone()
        .unwrap_or_else(|| format!("{title} {headline}"));
    notify::notify_in_app(id, format!("{title} {headline}"), body, cx);
}

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
            ferrumterm_ui::init(cx);
            ferrumgit_ui::init(cx);
            let view = TermView::new(TermViewOptions::standalone(), window, cx);
            cx.subscribe(&view, |_, event, cx| on_view_event(event, cx))
                .detach();
            view
        },
    );
}

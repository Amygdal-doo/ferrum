//! FerrumTerm: a GPUI terminal window on top of the persistent FerrumTerm
//! server. Tabs are the session's windows, splits are its blocks. Close the
//! window any time — agents keep running; reopen (or `fterm attach` from
//! anywhere, even over SSH) to pick up where you left off.

mod app;
mod dialogs;
mod input;
mod selection;
mod sidebar;
mod ssh;
mod ui_state;
mod view;

use app::App;
use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;
use view::TermView;

fn main() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env().unwrap_or_else(|_| "warn".into()),
        )
        .init();
    let runtime =
        match tokio::runtime::Builder::new_multi_thread().worker_threads(2).enable_all().build() {
            Ok(runtime) => runtime,
            Err(error) => {
                eprintln!("FerrumTerm: {error}");
                std::process::exit(1);
            }
        };
    let (sender, receiver) = futures::channel::mpsc::unbounded();
    let app = App::new(runtime, sender);

    ferrum_ui::run(
        AppConfig::new("FerrumTerm").size(1100., 700.).min_size(420., 260.),
        move |window, cx| cx.new(|cx| TermView::new(app, receiver, window, cx)),
    );
}

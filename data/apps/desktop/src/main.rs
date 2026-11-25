//! FerrumData desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod app;
mod app_info;
mod commands;
mod components;
mod search;
mod state;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;
use ferrumdata_application::AppServices;
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    let Some(dirs) = directories::ProjectDirs::from("dev", "FerrumData", "FerrumData") else {
        eprintln!("FerrumData: could not determine a home directory for local state");
        std::process::exit(1);
    };
    let services = match AppServices::start(dirs.data_dir()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FerrumData failed to start: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(data_dir = %dirs.data_dir().display(), vault = services.connections.vault_name(), "starting FerrumData");

    let config = AppConfig::new("FerrumData")
        .size(1360., 860.)
        .min_size(900., 560.)
        .app(app_info::app_info())
        .unified_titlebar();
    ferrum_ui::run(config, move |window, cx| {
        ferrum_account_ui::init(ferrum_account_ui::AccountApp::Data, cx);
        ferrumagent_ui::init(cx);
        ferrum_ui::search::bind_keys(cx);
        cx.new(|cx| app::FerrumData::new(services, window, cx))
    });
}

/// Logs never include SQL text, row values or secrets: only ids, states and
/// sanitized error messages. Override verbosity with `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new(
            "warn,ferrumdata_desktop=info,ferrumdata_application=info,ferrumdata_db_postgres=info",
        )
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

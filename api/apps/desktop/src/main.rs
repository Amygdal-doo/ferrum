//! FerrumAPI desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod commands;
mod components;
mod state;

use ferrum_ui::AppConfig;
use ferrumapi_application::AppServices;
use gpui::AppContext as _;
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    let Some(dirs) = directories::ProjectDirs::from("dev", "FerrumAPI", "FerrumAPI") else {
        eprintln!("FerrumAPI: could not determine a home directory for local state");
        std::process::exit(1);
    };
    let services = match AppServices::start(dirs.data_dir()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FerrumAPI failed to start: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(data_dir = %dirs.data_dir().display(), "starting FerrumAPI");

    ferrum_ui::run(AppConfig::new("FerrumAPI"), move |window, cx| {
        app::bind_keys(cx);
        cx.new(|cx| app::FerrumApi::new(services, window, cx))
    });
}

/// Logs never include URLs with credentials, header values, bodies or
/// secrets: only ids, methods, status codes and sanitized errors.
/// Override verbosity with `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,ferrumapi_desktop=info,ferrumapi_application=info,ferrumapi_http=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

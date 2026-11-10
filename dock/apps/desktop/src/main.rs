//! FerrumDock desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod commands;
mod services;
mod state;
mod views;

use ferrum_ui::AppConfig;
use gpui::AppContext as _;
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    let services = match services::Services::start() {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FerrumDock failed to start: {e}");
            std::process::exit(1);
        }
    };
    let docker_cli = commands::containers::detect_cli();
    let config = AppConfig::new("FerrumDock")
        .size(1440., 880.)
        .min_size(980., 560.);
    ferrum_ui::run(config, move |window, cx| {
        app::bind_keys(cx);
        cx.new(|cx| app::FerrumDock::new(services, docker_cli, window, cx))
    });
}

/// Logs never include environment values or log contents: only ids, operations and errors.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,ferrumdock=info,ferrumdock_engine=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

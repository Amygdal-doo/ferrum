//! FerrumContainers desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod agent;
mod app;
mod app_info;
mod commands;
mod project;
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
            eprintln!("FerrumContainers failed to start: {e}");
            std::process::exit(1);
        }
    };
    let docker_cli = commands::containers::detect_cli();
    // `ferrumdock <folder>` opens that project (like the other Ferrum apps).
    let open_at = std::env::args_os()
        .nth(1)
        .map(std::path::PathBuf::from)
        .filter(|p| p.is_dir())
        .map(|p| std::fs::canonicalize(&p).unwrap_or(p));
    let config = AppConfig::new("FerrumContainers")
        .size(1440., 880.)
        .min_size(980., 560.)
        .unified_titlebar()
        .app(app_info::app_info());
    ferrum_ui::run(config, move |window, cx| {
        ferrum_account_ui::init(ferrum_account_ui::AccountApp::Dock, cx);
        ferrumagent_ui::init(cx);
        app::bind_keys(cx);
        cx.new(|cx| {
            let mut app = app::FerrumContainers::new(services, docker_cli, window, cx);
            if let Some(path) = open_at {
                app.open_project(path, cx);
            }
            app
        })
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

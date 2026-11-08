//! Ferrum desktop entry point.

mod app;
mod commands;
mod components;
mod state;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use ferrum_application::AppServices;
use tracing_subscriber::EnvFilter;

/// Compiled Tailwind output, embedded so `cargo run` works without `dx`.
/// Regenerate after changing classes: see README "Styling".
const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");

fn main() {
    init_tracing();
    let Some(dirs) = directories::ProjectDirs::from("dev", "Ferrum", "Ferrum") else {
        eprintln!("Ferrum: could not determine a home directory for local state");
        std::process::exit(1);
    };
    let services = match AppServices::start(dirs.data_dir()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Ferrum failed to start: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(data_dir = %dirs.data_dir().display(), vault = services.connections.vault_name(), "starting Ferrum");

    let window = WindowBuilder::new()
        .with_title("Ferrum")
        .with_inner_size(LogicalSize::new(1360.0, 860.0))
        .with_min_inner_size(LogicalSize::new(900.0, 560.0))
        .with_always_on_top(false);
    let config = Config::new()
        .with_window(window)
        .with_data_directory(dirs.cache_dir().join("webview"))
        .with_custom_head(format!("<style>{TAILWIND_CSS}</style>"));

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(services)
        .launch(app::App);
}

/// Logs never include SQL text, row values or secrets: only ids, states and
/// sanitized error messages. Override verbosity with `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,ferrum_desktop=info,ferrum_application=info,ferrum_db_postgres=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

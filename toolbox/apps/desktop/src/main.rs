//! FerrumToolbox desktop entry point.

mod app;
mod commands;
mod components;
mod services;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;
use ferrumtoolbox_core::{Paths, State};
use services::Services;
use tracing_subscriber::EnvFilter;

fn main() {
    init_tracing();
    let Some(paths) = Paths::default_for_user() else {
        eprintln!("FerrumToolbox: could not determine a home directory for local state");
        std::process::exit(1);
    };
    let state = match State::load(&paths) {
        Ok(state) => state,
        Err(e) => {
            eprintln!(
                "FerrumToolbox: could not read {}: {e}",
                paths.state_file().display()
            );
            std::process::exit(1);
        }
    };
    let services = match Services::start(paths) {
        Ok(services) => services,
        Err(e) => {
            eprintln!("FerrumToolbox failed to start: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(data_dir = %services.paths.data_dir.display(), "starting FerrumToolbox");

    let config = AppConfig::new("FerrumToolbox")
        .size(1040., 680.)
        .min_size(760., 480.);
    ferrum_ui::run(config, move |window, cx| {
        cx.new(|cx| app::FerrumToolbox::new(services, state, window, cx))
    });
}

/// Logs never include tokens. Override verbosity with `RUST_LOG`.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn,ferrum_toolbox=info,ferrumtoolbox_core=info"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

//! FerrumGIT desktop entry point.

mod app;
mod commands;
mod components;
mod state;

use ferrum_ui::AppConfig;
use ferrumgit_application::AppServices;
use gpui::AppContext as _;
use tracing_subscriber::EnvFilter;

fn main() {
    // The same binary answers ssh's passphrase prompts (SSH_ASKPASS).
    if let Some(code) = ferrumgit_application::askpass_main() {
        std::process::exit(code);
    }
    init_tracing();
    let Some(dirs) = directories::ProjectDirs::from("dev", "FerrumGIT", "FerrumGIT") else {
        eprintln!("FerrumGIT: could not determine a home directory for local state");
        std::process::exit(1);
    };
    // `FERRUMGIT_DATA_DIR` isolates local state (tests, demos, multiple setups).
    let data_dir = std::env::var_os("FERRUMGIT_DATA_DIR")
        .map_or_else(|| dirs.data_dir().to_path_buf(), std::path::PathBuf::from);
    let services = match AppServices::start(&data_dir) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("FerrumGIT failed to start: {e}");
            std::process::exit(1);
        }
    };
    tracing::info!(data_dir = %data_dir.display(), "starting FerrumGIT");

    let config = AppConfig::new("FerrumGIT")
        .size(1440., 900.)
        .min_size(980., 600.);
    ferrum_ui::run(config, move |window, cx| {
        app::bind_keys(cx);
        cx.new(|cx| app::FerrumGit::new(services, window, cx))
    });
}

/// Logs never include file contents, commit messages, URLs with credentials
/// or passphrases: only ids, operation names and sanitized errors.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,ferrumgit=info,ferrumgit_application=info,ferrumgit_git=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

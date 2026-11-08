//! FerrumGIT desktop entry point.

mod app;
mod commands;
mod components;
mod state;

use dioxus::desktop::{Config, LogicalSize, WindowBuilder};
use ferrumgit_application::AppServices;
use tracing_subscriber::EnvFilter;

/// Compiled Tailwind output, embedded so `cargo run` works without `dx`.
/// Regenerate after changing classes: see README "Styling".
const TAILWIND_CSS: &str = include_str!("../assets/tailwind.css");

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

    let window = WindowBuilder::new()
        .with_title("FerrumGIT")
        .with_inner_size(LogicalSize::new(1440.0, 900.0))
        .with_min_inner_size(LogicalSize::new(980.0, 600.0))
        .with_always_on_top(false);
    // Tower-style unified titlebar: the toolbar draws under the traffic lights.
    #[cfg(target_os = "macos")]
    let window = {
        use dioxus::desktop::tao::platform::macos::WindowBuilderExtMacOS;
        window
            .with_titlebar_transparent(true)
            .with_fullsize_content_view(true)
            .with_title_hidden(true)
    };
    let config = Config::new()
        .with_window(window)
        .with_data_directory(dirs.cache_dir().join("webview"))
        .with_custom_head(format!("<style>{TAILWIND_CSS}</style>"));

    dioxus::LaunchBuilder::desktop()
        .with_cfg(config)
        .with_context(services)
        .launch(app::App);
}

/// Logs never include file contents, commit messages, URLs with credentials
/// or passphrases: only ids, operation names and sanitized errors.
fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        EnvFilter::new("warn,ferrumgit_desktop=info,ferrumgit_application=info,ferrumgit_git=info")
    });
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

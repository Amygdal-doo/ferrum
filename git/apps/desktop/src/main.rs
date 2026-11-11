//! FerrumGIT desktop entry point.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

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
    let open_at = repo_argument(std::env::args_os().skip(1));

    let config = AppConfig::new("FerrumGIT")
        .size(1440., 900.)
        .min_size(980., 600.);
    ferrum_ui::run(config, move |window, cx| {
        app::bind_keys(cx);
        cx.new(|cx| {
            let mut app = app::FerrumGit::new(services, window, cx);
            if let Some(path) = open_at {
                app.open_path(path, cx);
            }
            app
        })
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

/// `ferrumgit [path]`: the repository to open at launch (e.g. `ferrumgit .`
/// from a project root). Flags are skipped, including macOS's legacy `-psn_…`.
fn repo_argument(mut args: impl Iterator<Item = std::ffi::OsString>) -> Option<std::path::PathBuf> {
    let arg = args.find(|arg| !arg.to_string_lossy().starts_with('-'))?;
    let path = std::path::PathBuf::from(arg);
    Some(std::path::absolute(&path).unwrap_or(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsString;

    #[test]
    fn reads_the_first_path_argument() {
        let args = |list: &[&str]| {
            list.iter()
                .map(OsString::from)
                .collect::<Vec<_>>()
                .into_iter()
        };
        assert_eq!(repo_argument(args(&[])), None);
        assert_eq!(repo_argument(args(&["-psn_0_123"])), None);
        assert_eq!(
            repo_argument(args(&["-psn_0_1", "/repos/app", "/other"])),
            Some(std::path::PathBuf::from("/repos/app"))
        );
        assert!(repo_argument(args(&["relative"])).is_some_and(|p| p.is_absolute()));
    }
}

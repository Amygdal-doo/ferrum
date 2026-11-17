//! FerrumEngineer desktop entry point: an agent always live in a project,
//! with its threads in a sidebar and FerrumGIT and FerrumTerm beside it.
//! `ferrumvibe [folder]` opens on that project.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod app_info;
mod automations;
mod chrome;
mod columns;
mod editor;
mod extensions;
mod files;
mod layout;
mod menus;
mod nav;
mod navigate;
mod pages;
mod panes;
mod shell_path;
mod sidebar;
mod sites;
#[cfg(test)]
mod tests;
mod threads;

use std::path::PathBuf;
use std::rc::Rc;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;
use tracing_subscriber::EnvFilter;

fn main() {
    move_legacy_data();
    // The same binary answers ssh's passphrase prompts for FerrumGIT.
    if let Some(code) = ferrumgit_ui::askpass_main() {
        std::process::exit(code);
    }
    init_tracing();
    // The login shell's PATH (for agents and MCP servers), read while the
    // window opens.
    let path_env = std::thread::spawn(shell_path::login_shell_path);
    let project = folder_argument(std::env::args_os().skip(1)).or_else(app::last_project);

    let config = AppConfig::new("FerrumEngineer")
        .unified_titlebar()
        .size(1560., 920.)
        .min_size(720., 480.)
        .app(app_info::app_info());
    ferrum_ui::run(config, move |window, cx| {
        ferrum_account_ui::init(ferrum_account_ui::AccountApp::Vibe, cx);
        ferrumagent_ui::init(cx);
        ferrumgit_ui::init(cx);
        ferrumterm_ui::init(cx);
        app::bind_keys(cx);
        cx.set_menus(menus::menus(&app_info::app_info()));
        let path_env = path_env.join().ok().flatten();
        let options = app::Options {
            project,
            chat_options: Rc::new(move |_| ferrumagent_ui::ChatOptions {
                path_env: path_env.clone(),
                ..Default::default()
            }),
            ..app::Options::default()
        };
        cx.new(|cx| app::FerrumEngineer::new(options, window, cx))
    });
}

fn init_tracing() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));
    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .with_target(false)
        .init();
}

/// `ferrumvibe [folder]`: the first argument that is an existing
/// folder (flags such as macOS's `-psn_…` are skipped).
fn folder_argument(mut args: impl Iterator<Item = std::ffi::OsString>) -> Option<PathBuf> {
    let arg = args.find(|arg| !arg.to_string_lossy().starts_with('-'))?;
    std::fs::canonicalize(PathBuf::from(arg))
        .ok()
        .filter(|p| p.is_dir())
}

/// FerrumAgentic kept its files in `~/.ferrum/agentic` and
/// `~/.ferrum/ferrumagentic`; FerrumEngineer takes them over on its first start.
fn move_legacy_data() {
    for (old, new) in [("agentic", "vibe"), ("ferrumagentic", "ferrumvibe")] {
        if let (Some(old), Some(new)) = (ferrum_ui::prefs::path(old), ferrum_ui::prefs::path(new))
            && old.is_dir()
            && !new.exists()
            && let Err(error) = std::fs::rename(&old, &new)
        {
            tracing::warn!(%error, "could not move {} to {}", old.display(), new.display());
        }
    }
}

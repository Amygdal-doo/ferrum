//! Ferrum Editor desktop entry point: `ferrumeditor [path[:line[:col]]...]`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod ci;
mod debug;
mod editor;
mod file_icons;
mod find_bar;
mod logs;
mod lsp_store;
mod markdown_preview;
mod picker;
mod project_find;
mod registry;
mod settings;
mod theme_apply;
mod tree_view;
mod workspace;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ci;
#[cfg(test)]
mod tests_debug;
#[cfg(test)]
mod tests_logs;
#[cfg(test)]
mod tests_lsp;
#[cfg(test)]
mod tests_themes;
#[cfg(test)]
mod tests_tools;

use ferrum_ui::AppConfig;
use ferrumeditor_core::cli::{self, CliAction};
use ferrumeditor_core::paths::UserFiles;
use gpui::AppContext as _;

use settings::Settings;
use workspace::Workspace;

/// Key bindings of the embedded Git, Term and Agent panes.
pub fn init_tools(cx: &mut gpui::App) {
    ferrumgit_ui::init(cx);
    ferrumterm_ui::init(cx);
    ferrumagent_ui::init(cx);
}

/// Release builds get the version from `FERRUM_VERSION` at compile time.
pub const VERSION: &str = match option_env!("FERRUM_VERSION") {
    Some(version) => version,
    None => env!("CARGO_PKG_VERSION"),
};

fn main() {
    // SSH passphrase prompts of the Git pane re-run this binary as
    // SSH_ASKPASS: answer and exit before anything else starts.
    if let Some(code) = ferrumgit_ui::askpass_main() {
        std::process::exit(code);
    }
    let cwd = std::env::current_dir().unwrap_or_default();
    let targets = match cli::parse_args(std::env::args().skip(1), &cwd) {
        CliAction::Help => {
            println!("{}", cli::HELP);
            return;
        }
        CliAction::Version => {
            println!("ferrumeditor {VERSION}");
            return;
        }
        CliAction::Run(targets) => targets,
    };
    let config = AppConfig::new("Ferrum Editor")
        .size(1280., 820.)
        .min_size(640., 400.);
    ferrum_ui::run(config, move |window, cx| {
        // The embedded tools' key bindings (scoped to their panes) must be
        // in place before the base bindings are captured.
        init_tools(cx);
        actions::capture_base_bindings(cx);
        let settings = Settings::load(UserFiles::locate(), Settings::default_commands());
        let mut problems = settings.problems.clone();
        let theme = settings.theme.clone();
        let restore = settings.config.restore_session;
        problems.extend(actions::apply_keymap(&settings.keymap, cx));
        cx.set_global(settings);
        // Also holds task problems; servers only start if `[lsp] enabled`.
        let store = cx.new(lsp_store::LspStore::new);
        cx.set_global(lsp_store::GlobalLsp(store));
        theme_apply::apply(&theme, cx);
        actions::set_menus(cx);
        let workspace = cx.new(|cx| {
            let mut workspace = Workspace::new(window, cx);
            if targets.is_empty() {
                if restore {
                    workspace.restore_session(window, cx);
                }
            } else {
                workspace.open_targets(targets, window, cx);
            }
            if let Some(first) = problems.first() {
                workspace.error(
                    format!("{first} (+{} more)", problems.len() - 1).replace(" (+0 more)", ""),
                    cx,
                );
            }
            workspace
        });
        let weak = workspace.downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            // Quit asks about unsaved changes and saves the session.
            weak.update(cx, |workspace, cx| workspace.quit(window, cx))
                .is_err()
        });
        workspace
    });
}

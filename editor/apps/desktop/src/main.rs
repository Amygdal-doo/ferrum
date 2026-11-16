//! FerrumIDE desktop entry point: `ferrumeditor [path[:line[:col]]...]`.
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod actions;
mod app_info;
mod ci;
mod debug;
mod editor;
mod file_icons;
mod file_viewer;
mod find_bar;
mod instance;
mod logs;
mod lsp_store;
mod markdown_preview;
mod picker;
mod project_find;
mod registry;
mod settings;
mod settings_page;
mod theme_apply;
mod tree_view;
mod windows;
mod workspace;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_ci;
#[cfg(test)]
mod tests_debug;
#[cfg(test)]
mod tests_debug_lldb;
#[cfg(test)]
mod tests_drop;
#[cfg(test)]
mod tests_inline;
#[cfg(test)]
mod tests_logs;
#[cfg(test)]
mod tests_lsp;
#[cfg(test)]
mod tests_themes;
#[cfg(test)]
mod tests_tools;
#[cfg(test)]
mod tests_view;
#[cfg(test)]
mod tests_viewer;
#[cfg(test)]
mod tests_windows;

use ferrum_ui::AppConfig;
use ferrumeditor_core::cli::{self, CliAction};
use ferrumeditor_core::paths::UserFiles;
use futures::StreamExt as _;
use gpui::AppContext as _;

use settings::Settings;

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
    // One process: a running FerrumIDE opens the paths (in the window of
    // their project, or a new one) and this call ends here.
    let single = std::env::var_os(instance::NEW_INSTANCE_ENV).is_none_or(|v| v.is_empty());
    let socket = instance::socket_path().filter(|_| single);
    if let Some(socket) = &socket
        && instance::forward(socket, &targets)
    {
        return;
    }
    let (requests, mut incoming) = futures::channel::mpsc::unbounded();
    if let Some(socket) = &socket
        && let Err(error) = instance::listen(socket, requests.clone())
    {
        eprintln!("ferrumeditor: not listening for other launches: {error}");
    }
    let from_platform = requests;
    let config = AppConfig::new("FerrumIDE")
        .unified_titlebar()
        .size(1280., 820.)
        .min_size(640., 400.)
        .app(app_info::app_info())
        // Finder's Open With and drops on the Dock icon.
        .on_open_urls(move |urls| {
            let targets = windows::targets_from_urls(&urls);
            if !targets.is_empty() {
                from_platform.unbounded_send(targets).ok();
            }
        });
    ferrum_ui::run(config, move |window, cx| {
        ferrum_account_ui::init(ferrum_account_ui::AccountApp::Editor, cx);
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
        // The suite's colour scheme (Settings › Interface) picks between the
        // editor's own light and dark built-ins; a theme chosen in the editor
        // ("Theme: Select") always wins.
        ferrum_ui::appearance::set_theme_hook(
            std::rc::Rc::new(|prefs, appearance, cx| {
                theme_apply::follow_appearance(prefs, appearance, cx);
                true
            }),
            cx,
        );
        actions::set_menus(cx);
        // Paths from later launches and from the platform.
        cx.spawn(async move |cx| {
            while let Some(targets) = incoming.next().await {
                cx.update(|cx| windows::route(targets, cx));
            }
        })
        .detach();
        if let Some(socket) = socket {
            cx.on_app_quit(move |_| {
                instance::cleanup(&socket);
                async {}
            })
            .detach();
        }
        windows::build_workspace(targets, restore, &problems, window, cx)
    });
}

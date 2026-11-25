//! FerrumGIT's interface as an embeddable GPUI view.
//!
//! The FerrumGIT app is a window around one [`GitView`]; other Ferrum apps
//! (FerrumIDE, FerrumEngineer) show the same view in a split pane:
//!
//! ```ignore
//! // At startup (once per process):
//! if let Some(code) = ferrumgit_ui::askpass_main() { std::process::exit(code) }
//! ferrumgit_ui::init(cx);
//! // In the pane:
//! let git = GitView::new(Some(project_root), GitViewOptions::embedded(), window, cx);
//! cx.subscribe(&git, |_, _, event: &GitViewEvent, cx| { /* OpenFile … */ }).detach();
//! ```

mod app;
mod commands;
mod components;
mod embed;
mod folder;
mod search;
mod state;
#[cfg(test)]
mod tests;

pub use app::{GitView, GoBack, GoForward, KEY_CONTEXT, PICKER_CONTEXT, QuickOpen, Reload};
/// The working-tree file editor (FerrumIDE's editing core under a light
/// view), for hosts that show files in their own pane (FerrumEngineer's
/// Editor column). Its keys are bound by [`init`].
pub use components::editor::{CodeEditor, CodeEditorEvent};
pub use components::shell::layout::{MIN_WIDTH, WIDE_LAYOUT_MIN_WIDTH};
pub use embed::{
    GitServices, GitViewEvent, GitViewOptions, HistoryMarker, RemoteCommand, RepoSummary,
    RestoreMarker, default_data_dir, init, init_with_services,
};
pub use ferrumgit_application::AppServices;
/// SSH passphrase prompts re-run the host executable as `SSH_ASKPASS`:
/// call this first thing in `main` and exit with its code when it returns one.
pub use ferrumgit_application::askpass_main;

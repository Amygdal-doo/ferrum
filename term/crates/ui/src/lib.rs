//! FerrumTerm's terminal UI as an embeddable GPUI view.
//!
//! [`TermView`] is the whole FerrumTerm window content — tab bar, split
//! panes painted from server frames, the Agents / sessions / SSH-hosts
//! sidebar, and the host-form and ports dialogs. The FerrumTerm desktop app
//! is a thin binary around it; FerrumIDE and FerrumEngineer embed it as a
//! split pane:
//!
//! ```ignore
//! ferrumterm_ui::init(cx); // once, at startup: key bindings
//! let term = TermView::new(
//!     TermViewOptions::embedded().cwd(project_root).session_name("editor:my-app"),
//!     window,
//!     cx,
//! );
//! cx.subscribe(&term, |_, _, event: &TermViewEvent, _| { /* … */ }).detach();
//! term.update(cx, |term, cx| term.run_command("cargo test", None, cx));
//! ```
//!
//! Terminals live in the persistent `fterm` server (started on demand), so
//! they survive the host app closing. When that server can't be reached or
//! started — or with `attach_to_server: false` — the view runs a private
//! server inside the host process instead (sessions then end with the host).

mod actions;
mod agent_pane;
mod agent_tools;
mod alerts;
mod app;
mod app_info;
#[cfg(test)]
mod app_tests;
mod connection;
mod dialogs;
mod events;
mod find;
mod folders;
mod git_pane;
mod input;
mod options;
mod paint;
mod project;
mod selection;
pub mod settings;
mod settings_keyboard;
mod settings_machines;
mod settings_pages;
mod sidebar;
mod ssh;
mod tab_style;
mod ui_state;
mod view;
mod worktree;

pub use actions::{
    ActivateTab1, ActivateTab2, ActivateTab3, ActivateTab4, ActivateTab5, ActivateTab6,
    ActivateTab7, ActivateTab8, ActivateTab9, ClosePane, Copy, CopyScreen, DecreaseFontSize, Find,
    FindNext, FindPrevious, IncreaseFontSize, KEY_CONTEXT, NewSession, NewTab, NextPane,
    NextSession, NextTab, Paste, PreviousPane, PreviousTab, ResetFontSize, ScrollPageDown,
    ScrollPageUp, SplitDown, SplitRight, ToggleSidebar, key_bindings,
};
pub use app_info::app_info;
pub use connection::Backend;
pub use events::{CommandId, TermViewEvent};
pub use ferrumterm_protocol::{BlockId, ProgramState, ProgramStatus};
pub use options::TermViewOptions;
pub use settings_keyboard::KeyboardPage;
pub use settings_machines::MachinesPage;
pub use settings_pages::TerminalPage;
pub use view::TermView;

use ferrum_ui::gpui::{App, Global};

struct Initialized;
impl Global for Initialized {}

/// Registers FerrumTerm's key bindings (⌘T, ⌘D, ⌘W, ⌘B…) in the
/// [`KEY_CONTEXT`] key context, so they apply only while a terminal has
/// focus and win over the host's global bindings there. Call once at
/// startup; later calls do nothing.
pub fn init(cx: &mut App) {
    if cx.has_global::<Initialized>() {
        return;
    }
    cx.set_global(Initialized);
    cx.bind_keys(key_bindings());
}

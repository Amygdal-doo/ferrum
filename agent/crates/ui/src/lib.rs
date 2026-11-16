//! FerrumChat's GPUI view: a Cursor-like agentic coding chat that a host
//! embeds as a pane (FerrumIDE's split, a FerrumEngineer column).
//!
//! ```ignore
//! ferrumagent_ui::init(cx); // once, after ferrum_ui's theme
//! let chat = ChatView::new(project_root, ChatOptions::default(), window, cx);
//! cx.subscribe(&chat, |_, event, cx| match event {
//!     ChatEvent::OpenFile { path, line } => { /* open it */ }
//!     ChatEvent::RunInTerminal { cmd, cwd } => { /* run it */ }
//!     ChatEvent::ChangesUpdated { files } => { /* refresh views */ }
//!     ChatEvent::ThreadsChanged => { /* reload a thread sidebar */ }
//!     ChatEvent::Git(request) => { /* push, pull, commit… */ }
//!     ChatEvent::StatusChanged => { /* redraw thread status dots */ }
//! });
//! // Hidden chats keep running; tell the chat so finished turns count as
//! // unseen (Done) and notify:
//! chat.update(cx, |chat, cx| chat.set_visible(false, cx));
//! ```
//!
//! A host that draws its own thread list and window chrome sets
//! [`ChatOptions::shell`] and drives the chat with
//! [`ChatView::open_thread`], [`ChatView::start_task`] and
//! [`ChatView::set_git_status`].
//!
//! Everything below the pixels lives in `ferrumagent-core`.

mod actions;
mod attach;
mod changes;
mod composer;
mod custom_form;
mod footer;
mod header;
mod host;
mod live;
mod markdown;
mod menus;
mod messages;
mod settings;
mod threads;
mod tool_card;
mod view;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_models;

pub use actions::*;
pub use ferrumagent_core as core;
pub use ferrumagent_core::permissions::Approval;
pub use ferrumagent_core::session::McpServerStatus;
pub use host::{GitRequest, GitStatus};
pub use live::{
    DONE, DONE_EVENT, FAILED, FAILED_EVENT, NEEDS_YOU, NEEDS_YOU_EVENT, Notifier, ToastClick,
    default_notifier,
};
pub use threads::{status_indicator, thread_logo};
pub use view::{ChatEvent, ChatOptions, ChatView, HeaderSlot};

use gpui::{App, KeyBinding};

/// The key context of the whole chat (`FerrumChat`) and of its composer
/// (`FerrumChatComposer`).
pub const KEY_CONTEXT: &str = "FerrumChat";
pub const COMPOSER_CONTEXT: &str = "FerrumChatComposer";

/// Registers FerrumChat's key bindings. Call once after `ferrum_ui` is
/// initialised (bindings registered later win over the text input's).
pub fn init(cx: &mut App) {
    let chat = Some(KEY_CONTEXT);
    let input = Some("FerrumChatComposer > FerrumTextInput");
    cx.bind_keys([
        KeyBinding::new("enter", Send, input),
        KeyBinding::new("shift-enter", NewLine, input),
        KeyBinding::new("up", MentionPrev, input),
        KeyBinding::new("down", MentionNext, input),
        KeyBinding::new("tab", AcceptMention, input),
        KeyBinding::new("shift-tab", CycleMode, chat),
        KeyBinding::new("escape", Dismiss, chat),
        KeyBinding::new("cmd-.", Stop, chat),
        KeyBinding::new("cmd-alt-n", NewThread, chat),
        KeyBinding::new("cmd-alt-j", ToggleThreads, chat),
        KeyBinding::new("cmd-shift-enter", AcceptAll, chat),
        KeyBinding::new("cmd-shift-backspace", RejectAll, chat),
        KeyBinding::new("cmd-alt-r", ToggleChanges, chat),
    ]);
}

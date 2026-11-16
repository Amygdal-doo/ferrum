//! The Agent pane of an app that is not a code editor (FerrumTerm,
//! FerrumData, FerrumContainers, FerrumSystem): the top-right toggle, with
//! FerrumIDE's icon and tooltip, and the chat in a card beside the app's
//! own, created the first time it opens and kept (with its running turns)
//! while hidden.
//!
//! ```ignore
//! // In the title row, just before the account button:
//! row.child(ferrumagent_ui::agent_toggle("toggle-agent", self.agent.is_shown())
//!     .on_click(cx.listener(|this, _, window, cx| {
//!         this.agent.toggle(window, cx, |window, cx| {
//!             ChatView::without_project(host(), AgentPane::options(None), window, cx)
//!         });
//!         cx.notify();
//!     })));
//! // Beside the app's cards:
//! row.children(self.agent.card(cx))
//! ```

use ferrum_ui::prelude::*;
use gpui::{AnyElement, Entity, Window};

use crate::view::{ChatOptions, ChatView, HostContextFn};

/// Narrowest the pane gets (the chat works from 320 px).
pub const MIN_WIDTH: f32 = 340.;
/// Share of the window the pane takes.
const SHARE: f32 = 0.34;

/// FerrumIDE's Agent toggle, for a title row (just before the account).
pub fn agent_toggle(id: impl Into<ElementId>, open: bool) -> IconButton {
    let tip = if open {
        "Hide Agent pane"
    } else {
        "Show Agent pane"
    };
    ferrum_ui::title_button(id, IconName::Sparkle, tip)
        .toggle_state(open)
        .when(open, |b| b.icon_color(Color::Accent))
}

/// The chat beside a host app's own cards.
#[derive(Default)]
pub struct AgentPane {
    chat: Option<Entity<ChatView>>,
    shown: bool,
}

impl AgentPane {
    /// Starts reading the login shell's `PATH` (for external agents and MCP
    /// servers), so it is there when the chat opens.
    pub fn new() -> Self {
        ferrumagent_core::shell_path::resolve_in_background();
        Self::default()
    }

    /// Options for the pane's chat: the login shell's `PATH` and `context`
    /// (what the user is looking at, read before each message).
    pub fn options(context: Option<HostContextFn>) -> ChatOptions {
        ChatOptions {
            path_env: ferrumagent_core::shell_path::resolved(),
            context,
            ..ChatOptions::default()
        }
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }

    pub fn chat(&self) -> Option<&Entity<ChatView>> {
        self.chat.as_ref()
    }

    /// Shows the pane — its chat made with `make` the first time — and
    /// focuses the composer, or hides it.
    pub fn toggle(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        make: impl FnOnce(&mut Window, &mut App) -> Entity<ChatView>,
    ) {
        if self.shown {
            self.hide(cx);
            return;
        }
        let chat = match self.chat.clone() {
            Some(chat) => chat,
            None => make(window, cx),
        };
        self.show(chat, window, cx);
    }

    /// Shows `chat` in the pane (a host with one chat per folder swaps
    /// them); the one shown before keeps running, hidden.
    pub fn show(&mut self, chat: Entity<ChatView>, window: &mut Window, cx: &mut App) {
        if let Some(old) = self.chat.as_ref().filter(|old| **old != chat) {
            old.update(cx, |old, cx| old.set_visible(false, cx));
        }
        chat.update(cx, |chat, cx| {
            chat.set_visible(true, cx);
            chat.focus_composer(window, cx);
        });
        self.chat = Some(chat);
        self.shown = true;
    }

    /// Hides the pane; the chat keeps running (finished turns notify).
    pub fn hide(&mut self, cx: &mut App) {
        self.shown = false;
        if let Some(chat) = &self.chat {
            chat.update(cx, |chat, cx| chat.set_visible(false, cx));
        }
    }

    /// The card, while shown: a share of the window, at least
    /// [`MIN_WIDTH`]. Put it in the row beside the app's cards.
    pub fn card(&self, cx: &App) -> Option<AnyElement> {
        let chat = self.chat.clone().filter(|_| self.shown)?;
        Some(
            ferrum_ui::pane_card(false, cx)
                .id("agent-pane")
                .h_full()
                .flex_basis(relative(SHARE))
                .flex_shrink_0()
                .min_w(px(MIN_WIDTH))
                .overflow_hidden()
                .child(div().size_full().child(chat))
                .into_any_element(),
        )
    }
}

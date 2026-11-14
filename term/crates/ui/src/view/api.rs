//! The embeddable API of [`TermView`]: what a host (Ferrum Editor,
//! FerrumVibe, the FerrumTerm desktop app) calls.
//!
//! Calls made before the view is connected and showing its session are
//! queued and run once it is.

use super::TermView;
use crate::connection::Backend;
use crate::events::CommandId;
use crate::options::TermViewOptions;
use ferrum_ui::gpui::{App, AppContext as _, Axis, Context, Entity, Window};
use ferrumterm_protocol::{Direction, Input, Request};
use std::path::{Path, PathBuf};

impl TermView {
    /// Creates the view and starts connecting (see [`TermViewOptions`]).
    /// Subscribe to [`crate::TermViewEvent`]s with `cx.subscribe`.
    pub fn new(options: TermViewOptions, window: &mut Window, cx: &mut App) -> Entity<Self> {
        cx.new(|cx| Self::build(options, window, cx))
    }

    /// Opens a tab with a shell in `cwd` (else the view's directory).
    pub fn new_tab(&mut self, cwd: Option<PathBuf>, cx: &mut Context<Self>) {
        self.app.when_ready(move |app| app.new_tab(cwd));
        self.after_change(cx);
    }

    /// Splits the focused pane: `Axis::Horizontal` puts the new pane to the
    /// right, `Axis::Vertical` below. The new pane gets focus.
    pub fn split(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let dir = match axis {
            Axis::Horizontal => Direction::Right,
            Axis::Vertical => Direction::Down,
        };
        self.app.when_ready(move |app| app.split(None, dir));
        self.after_change(cx);
    }

    /// Opens a tab running `command_line` through the user's login shell
    /// (so their PATH applies), in `cwd` (else the view's directory),
    /// labelled after its program. The tab stays open after the command
    /// exits. [`crate::TermViewEvent::CommandStarted`] and
    /// [`crate::TermViewEvent::CommandFinished`] carry the returned id.
    pub fn run_command(
        &mut self,
        command_line: &str,
        cwd: Option<PathBuf>,
        cx: &mut Context<Self>,
    ) -> CommandId {
        let id = self.app.next_command_id();
        let line = command_line.trim().to_owned();
        if !line.is_empty() {
            self.app
                .when_ready(move |app| app.run_command(id, &line, cwd));
        }
        self.after_change(cx);
        id
    }

    /// Sets the directory for tabs, splits and commands opened without one
    /// (and for the view's session, when it is created later). Running
    /// shells are left where they are.
    pub fn set_cwd(&mut self, cwd: impl Into<PathBuf>, cx: &mut Context<Self>) {
        self.app.default_cwd = Some(cwd.into());
        cx.notify();
    }

    /// The directory set with [`Self::set_cwd`] or [`TermViewOptions::cwd`].
    #[must_use]
    pub fn cwd(&self) -> Option<&Path> {
        self.app.default_cwd.as_deref()
    }

    /// Gives keyboard focus to the terminal (its focused pane).
    pub fn focus_active(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    /// Types `text` into the focused pane, as if pasted.
    pub fn send_text(&mut self, text: &str, cx: &mut Context<Self>) {
        let text = text.to_owned();
        self.app.when_ready(move |app| {
            let target = app.focused().map(|b| b.to_string());
            app.send(Request::Input {
                target,
                input: vec![Input::Paste(text)],
            });
        });
        self.after_change(cx);
    }

    /// Shows or hides the sidebar (agents, sessions, SSH hosts).
    pub fn set_sidebar_visible(&mut self, visible: bool, cx: &mut Context<Self>) {
        if self.app.ui.sidebar_open != visible {
            self.app.toggle_sidebar();
            cx.notify();
        }
    }

    #[must_use]
    pub fn sidebar_visible(&self) -> bool {
        self.app.ui.sidebar_open
    }

    /// The server this view is connected to (`None` while connecting).
    #[must_use]
    pub fn backend(&self) -> Option<Backend> {
        self.app.client.as_ref().and(self.app.backend)
    }

    #[must_use]
    pub fn options(&self) -> &TermViewOptions {
        &self.options
    }

    /// The focused pane's title and the session, e.g. `vim — editor:app`.
    #[must_use]
    pub fn title(&self) -> String {
        let Some(session) = self.app.session() else {
            return "FerrumTerm".to_owned();
        };
        let title = self
            .app
            .window()
            .and_then(|window| {
                window
                    .blocks
                    .iter()
                    .find(|block| block.id == window.focused)
            })
            .map(|block| block.title.clone())
            .unwrap_or_default();
        if title.is_empty() {
            format!("FerrumTerm — {}", session.name)
        } else {
            format!("{title} — {}", session.name)
        }
    }

    /// Columns × rows of the terminal area, once laid out.
    #[must_use]
    pub fn visible_grid(&self) -> Option<(u16, u16)> {
        self.grid
    }

    /// Number of tabs in the view's session.
    #[must_use]
    pub fn tab_count(&self) -> usize {
        self.app.session().map_or(0, |s| s.windows.len())
    }
}

//! How a [`crate::TermView`] is set up: standalone window or embedded pane.

use std::path::PathBuf;

/// Options for [`crate::TermView::new`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TermViewOptions {
    /// Directory for the view's first session and for tabs, splits and
    /// commands opened without one (see [`crate::TermView::set_cwd`]).
    /// `None`: the server's default (home).
    pub cwd: Option<PathBuf>,
    /// Embedded layout: smaller font and tab bar, scrollable tabs, and the
    /// sidebar drawn over the terminal instead of beside it. Usable from
    /// 320 px wide.
    pub compact: bool,
    /// Whether the sidebar (agents, sessions, SSH hosts) starts open. Ignored
    /// when `standalone`: the window remembers its own choice.
    pub show_sidebar: bool,
    /// Use the shared, persistent `fterm` server (started if needed), so
    /// terminals outlive the host app. When it can't be reached or started
    /// (embedded views; the standalone window keeps retrying), or with
    /// `false`, a private server runs inside the host process.
    pub attach_to_server: bool,
    /// Show (and create when missing) this server session instead of the
    /// first one, so a host keeps its terminals apart from FerrumTerm's own,
    /// e.g. `editor:<project>`.
    pub session_name: Option<String>,
    /// The FerrumTerm desktop window: sets the window title, shows the
    /// shared project bar, takes focus on creation, persists the sidebar's
    /// state, and retries the shared server instead of falling back to a
    /// private one. Hosts leave this `false`.
    pub standalone: bool,
    /// The Agent toggle and pane (FerrumChat beside the terminal; the
    /// desktop window). Hosts with their own agent leave this `false`.
    pub agent: bool,
    /// Where the Agent pane's chat keeps its settings and threads, with
    /// keys in memory (tests); `None`: `~/.ferrum/agent` and the keychain.
    pub agent_home: Option<PathBuf>,
}

impl Default for TermViewOptions {
    fn default() -> Self {
        Self {
            cwd: None,
            compact: false,
            show_sidebar: true,
            attach_to_server: true,
            session_name: None,
            standalone: false,
            agent: false,
            agent_home: None,
        }
    }
}

impl TermViewOptions {
    /// The FerrumTerm desktop window.
    #[must_use]
    pub fn standalone() -> Self {
        Self {
            standalone: true,
            agent: true,
            ..Self::default()
        }
    }

    /// The Agent toggle and pane; chats keep their settings, threads and
    /// keys in `home` instead of the user's (tests).
    #[must_use]
    pub fn agent(mut self, home: Option<PathBuf>) -> Self {
        self.agent = true;
        self.agent_home = home;
        self
    }

    /// A compact pane inside another app, sidebar collapsed.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            compact: true,
            show_sidebar: false,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn cwd(mut self, cwd: impl Into<PathBuf>) -> Self {
        self.cwd = Some(cwd.into());
        self
    }

    #[must_use]
    pub fn session_name(mut self, name: impl Into<String>) -> Self {
        self.session_name = Some(name.into());
        self
    }

    #[must_use]
    pub fn compact(mut self, compact: bool) -> Self {
        self.compact = compact;
        self
    }

    #[must_use]
    pub fn show_sidebar(mut self, show: bool) -> Self {
        self.show_sidebar = show;
        self
    }

    #[must_use]
    pub fn attach_to_server(mut self, attach: bool) -> Self {
        self.attach_to_server = attach;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presets() {
        let d = TermViewOptions::default();
        assert!(d.attach_to_server && d.show_sidebar && !d.compact && !d.standalone);
        assert!(TermViewOptions::standalone().standalone && TermViewOptions::standalone().agent);
        assert!(!d.agent && !TermViewOptions::embedded().agent);
        let e = TermViewOptions::embedded()
            .cwd("/code/app")
            .session_name("editor:app");
        assert!(e.compact && !e.show_sidebar && e.attach_to_server);
        assert_eq!(e.cwd.as_deref(), Some(std::path::Path::new("/code/app")));
        assert_eq!(e.session_name.as_deref(), Some("editor:app"));
    }
}

//! Which Ferrum app is signing in ([`App`]) and how an app relates to the
//! launcher's account ([`AppMode`]).

use serde::{Deserialize, Serialize};
use std::fmt;

/// A desktop app that can hold an account session. The server knows each
/// as `client_id = ferrum-<id>`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum App {
    Launcher,
    Editor,
    Vibe,
    Git,
    Term,
    Api,
    Dock,
    Data,
    Env,
    Ports,
    Kit,
    System,
}

impl App {
    pub const ALL: [App; 12] = [
        App::Launcher,
        App::Editor,
        App::Vibe,
        App::Git,
        App::Term,
        App::Api,
        App::Dock,
        App::Data,
        App::Env,
        App::Ports,
        App::Kit,
        App::System,
    ];

    /// The id used in `client_id`, the keychain account and `account.json`.
    #[must_use]
    pub const fn id(self) -> &'static str {
        match self {
            App::Launcher => "launcher",
            App::Editor => "editor",
            App::Vibe => "vibe",
            App::Git => "git",
            App::Term => "term",
            App::Api => "api",
            App::Dock => "dock",
            App::Data => "data",
            App::Env => "env",
            App::Ports => "ports",
            App::Kit => "kit",
            App::System => "system",
        }
    }

    /// `ferrum-<id>`.
    #[must_use]
    pub fn client_id(self) -> String {
        format!("ferrum-{}", self.id())
    }

    #[must_use]
    pub fn parse(id: &str) -> Option<App> {
        App::ALL.into_iter().find(|app| app.id() == id)
    }

    #[must_use]
    pub fn is_launcher(self) -> bool {
        self == App::Launcher
    }
}

impl fmt::Display for App {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.id())
    }
}

/// How an app (other than the launcher) picks its account. Stored per app
/// in `account.json`; a missing entry means [`AppMode::FollowLauncher`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppMode {
    /// Use the launcher's account (single sign-on via the `exchange` grant).
    #[default]
    FollowLauncher,
    /// Signed out on purpose: stays signed out even when the launcher is
    /// signed in.
    SignedOut,
    /// Signed in on its own, possibly as another user.
    OwnAccount,
}

/// What happened to an app's sign-in, for [`AppMode::after`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModeEvent {
    /// Signed in through the browser from the app itself.
    SignedInInBrowser,
    /// Signed in with the launcher's account (exchange grant), or the user
    /// chose "Use launcher account".
    UsedLauncherAccount,
    /// Signed out of this app only.
    SignedOutHere,
    /// Signed out everywhere: back to following the launcher, which is
    /// itself signed out.
    SignedOutEverywhere,
}

impl AppMode {
    /// The mode after `event`.
    #[must_use]
    pub fn after(self, event: ModeEvent) -> AppMode {
        match event {
            ModeEvent::SignedInInBrowser => AppMode::OwnAccount,
            ModeEvent::UsedLauncherAccount | ModeEvent::SignedOutEverywhere => {
                AppMode::FollowLauncher
            }
            ModeEvent::SignedOutHere => AppMode::SignedOut,
        }
    }

    /// Whether the app should sign itself in from the launcher's session
    /// when it has no session of its own.
    #[must_use]
    pub fn follows_launcher(self) -> bool {
        self == AppMode::FollowLauncher
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_client_ids_match_the_contract() {
        for app in App::ALL {
            assert_eq!(App::parse(app.id()), Some(app));
        }
        assert_eq!(App::Editor.client_id(), "ferrum-editor");
        assert_eq!(App::parse("web"), None);
    }

    #[test]
    fn mode_transitions() {
        use AppMode::*;
        use ModeEvent::*;
        for mode in [FollowLauncher, SignedOut, OwnAccount] {
            assert_eq!(mode.after(SignedInInBrowser), OwnAccount);
            assert_eq!(mode.after(UsedLauncherAccount), FollowLauncher);
            assert_eq!(mode.after(SignedOutHere), SignedOut);
            assert_eq!(mode.after(SignedOutEverywhere), FollowLauncher);
        }
        assert!(FollowLauncher.follows_launcher());
        assert!(!SignedOut.follows_launcher());
        assert!(!OwnAccount.follows_launcher());
    }

    #[test]
    fn modes_serialize_as_in_the_contract() {
        let json = serde_json::to_string(&[
            AppMode::FollowLauncher,
            AppMode::SignedOut,
            AppMode::OwnAccount,
        ])
        .expect("json");
        assert_eq!(json, r#"["follow_launcher","signed_out","own_account"]"#);
    }
}

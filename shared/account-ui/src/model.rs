//! What the account control offers in each state, without any UI: given the
//! app, its [`AppMode`], whether it is signed in and whether the launcher
//! has a session, [`Status`] decides which menu items and sign-in choices
//! show. The button, the menu and Settings › Account all follow it.

use ferrum_account::{App, AppMode};

/// The account as one app sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Status {
    pub app: App,
    pub mode: AppMode,
    /// This app has a user (cached or fetched).
    pub signed_in: bool,
    /// The launcher has a session (its user is cached in `account.json`).
    pub launcher_signed_in: bool,
}

/// One item (or group) of the avatar menu, top to bottom.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuItem {
    /// `@username`: opens the account page on the web.
    Profile,
    /// The "Organization" header and the organizations.
    Organizations,
    /// Settings (⌘,).
    Settings,
    /// Entries the app adds (Keymap, Themes, …), when it has any.
    AppEntries,
    /// Sign in through the browser as someone else (own account).
    SwitchAccount,
    /// Back to the launcher's account (single sign-on).
    UseLauncherAccount,
    /// "Sign out of <App>".
    SignOutHere,
    SignOutEverywhere,
}

impl MenuItem {
    /// Items of one group sit together; separators go between groups.
    #[must_use]
    pub fn group(self) -> u8 {
        match self {
            MenuItem::Profile => 0,
            MenuItem::Organizations => 1,
            MenuItem::Settings | MenuItem::AppEntries => 2,
            MenuItem::SwitchAccount | MenuItem::UseLauncherAccount => 3,
            MenuItem::SignOutHere | MenuItem::SignOutEverywhere => 4,
        }
    }
}

/// What clicking **Sign in** does while signed out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignInChoice {
    /// Straight to the browser (the app gets its own account).
    Browser,
    /// A menu: "Use launcher account" or sign in through the browser.
    LauncherOrBrowser,
}

impl Status {
    /// "Use launcher account" is offered when this app is not the
    /// launcher, the launcher has a session, and the app is signed out or
    /// on an account of its own (or following the launcher but not signed
    /// in yet, e.g. single sign-on failed).
    #[must_use]
    pub fn offers_launcher_account(&self) -> bool {
        !self.app.is_launcher()
            && self.launcher_signed_in
            && (!self.signed_in || self.mode != AppMode::FollowLauncher)
    }

    /// The avatar menu, while signed in (empty while signed out).
    #[must_use]
    pub fn menu(&self) -> Vec<MenuItem> {
        use MenuItem::*;
        if !self.signed_in {
            return Vec::new();
        }
        let mut items = vec![Profile, Organizations, Settings, AppEntries];
        if !self.app.is_launcher() {
            items.push(SwitchAccount);
        }
        if self.offers_launcher_account() {
            items.push(UseLauncherAccount);
        }
        items.extend([SignOutHere, SignOutEverywhere]);
        items
    }

    /// What **Sign in** does (only meaningful while signed out).
    #[must_use]
    pub fn sign_in_choice(&self) -> SignInChoice {
        if self.offers_launcher_account() {
            SignInChoice::LauncherOrBrowser
        } else {
            SignInChoice::Browser
        }
    }
}

#[cfg(test)]
mod tests {
    use super::MenuItem::*;
    use super::*;
    use ferrum_account::ModeEvent;

    fn status(app: App, mode: AppMode, signed_in: bool, launcher: bool) -> Status {
        Status {
            app,
            mode,
            signed_in,
            launcher_signed_in: launcher,
        }
    }

    #[test]
    fn launcher_menu_is_unchanged() {
        let s = status(App::Launcher, AppMode::FollowLauncher, true, true);
        assert_eq!(
            s.menu(),
            vec![
                Profile,
                Organizations,
                Settings,
                AppEntries,
                SignOutHere,
                SignOutEverywhere
            ]
        );
        assert!(!s.offers_launcher_account());
        let out = status(App::Launcher, AppMode::FollowLauncher, false, false);
        assert!(out.menu().is_empty());
        assert_eq!(out.sign_in_choice(), SignInChoice::Browser);
    }

    #[test]
    fn following_the_launcher_offers_switch_but_not_the_launcher_account() {
        let s = status(App::Editor, AppMode::FollowLauncher, true, true);
        assert_eq!(
            s.menu(),
            vec![
                Profile,
                Organizations,
                Settings,
                AppEntries,
                SwitchAccount,
                SignOutHere,
                SignOutEverywhere
            ]
        );
    }

    #[test]
    fn own_account_offers_the_launcher_account_when_it_has_a_session() {
        let own = status(App::Git, AppMode::OwnAccount, true, true);
        assert!(own.menu().contains(&UseLauncherAccount));
        let alone = status(App::Git, AppMode::OwnAccount, true, false);
        assert!(!alone.menu().contains(&UseLauncherAccount));
        assert!(alone.menu().contains(&SwitchAccount));
    }

    #[test]
    fn signing_out_here_then_back_through_the_launcher() {
        // Following the launcher, signed in; "Sign out of FerrumTerm".
        let mode = AppMode::FollowLauncher.after(ModeEvent::SignedOutHere);
        assert_eq!(mode, AppMode::SignedOut);
        // The launcher stays signed in, so Sign in offers its account.
        let out = status(App::Term, mode, false, true);
        assert!(out.menu().is_empty());
        assert_eq!(out.sign_in_choice(), SignInChoice::LauncherOrBrowser);
        // "Use launcher account" follows the launcher again.
        let back = mode.after(ModeEvent::UsedLauncherAccount);
        let s = status(App::Term, back, true, true);
        assert!(!s.menu().contains(&UseLauncherAccount));
    }

    #[test]
    fn without_a_launcher_session_sign_in_goes_to_the_browser() {
        for mode in [
            AppMode::FollowLauncher,
            AppMode::SignedOut,
            AppMode::OwnAccount,
        ] {
            let s = status(App::Data, mode, false, false);
            assert_eq!(s.sign_in_choice(), SignInChoice::Browser);
        }
        // Signing in in the browser gives the app its own account.
        assert_eq!(
            AppMode::SignedOut.after(ModeEvent::SignedInInBrowser),
            AppMode::OwnAccount
        );
    }

    #[test]
    fn following_without_a_session_yet_offers_the_launcher_account() {
        // E.g. the keychain prompt was denied at start.
        let s = status(App::Kit, AppMode::FollowLauncher, false, true);
        assert_eq!(s.sign_in_choice(), SignInChoice::LauncherOrBrowser);
    }

    #[test]
    fn signing_out_everywhere_follows_the_signed_out_launcher() {
        let mode = AppMode::OwnAccount.after(ModeEvent::SignedOutEverywhere);
        let s = status(App::Ports, mode, false, false);
        assert!(s.menu().is_empty());
        assert_eq!(s.sign_in_choice(), SignInChoice::Browser);
    }
}

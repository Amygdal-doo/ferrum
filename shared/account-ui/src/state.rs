//! [`AccountState`]: the app's Ferrum account as the UI sees it (user,
//! avatar, mode, whether the launcher has a session) and the actions behind
//! the button, the menu and Settings › Account. Keychain and network work
//! runs on [`crate::runtime`], never on the UI thread; tokens never reach
//! this module.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use ferrum_account::{Account, AccountError, App as AccountApp, AppMode, Me, Scope};
use ferrum_ui::gpui::Image;
use ferrum_ui::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::avatar::image_format;
use crate::model::Status;
use crate::runtime;

/// Avatar download size (the button is 20 px; 2× for Retina, plus room).
const AVATAR_PX: u32 = 64;
/// How often `account.json` is looked at for changes made by other apps
/// (the launcher signing in or out).
const POLL: Duration = Duration::from_secs(20);

/// Shows a message about the account (sign-in done, failures).
pub type MessageHandler = Rc<dyn Fn(SharedString, bool, &mut App)>;

/// `account.json` as last read: this app's mode and the launcher's user.
#[derive(Debug, Clone, PartialEq, Default)]
struct Snapshot {
    mode: AppMode,
    launcher_user: Option<Me>,
}

impl Snapshot {
    fn read(account: &Account) -> Self {
        Self {
            mode: account.mode().unwrap_or_default(),
            launcher_user: account.launcher_user().ok().flatten(),
        }
    }

    /// What, if it changes, calls for checking the session again.
    fn key(&self) -> (AppMode, Option<&str>) {
        (
            self.mode,
            self.launcher_user.as_ref().map(|u| u.id.as_str()),
        )
    }
}

pub struct AccountState {
    app: AccountApp,
    /// `None` without a home directory (the button is hidden).
    client: Option<Arc<Account>>,
    user: Option<Me>,
    snapshot: Snapshot,
    avatar: Option<Arc<Image>>,
    avatar_url: Option<String>,
    /// Set while the browser sign-in runs; cancelling stops the wait.
    signing_in: Option<CancellationToken>,
    /// A restore or "Use launcher account" in flight.
    checking: bool,
    on_message: Option<MessageHandler>,
    _poll: Option<Task<()>>,
}

impl AccountState {
    /// Loads `account.json` (the cached user shows right away); call
    /// [`Self::start`] to check the session in the background.
    pub(crate) fn new(app: AccountApp) -> Self {
        let client = match Account::load(app) {
            Ok(account) => Some(Arc::new(account)),
            Err(e) => {
                tracing::warn!(error = %e, "Ferrum account unavailable");
                None
            }
        };
        let user = client.as_ref().and_then(|a| a.current_user());
        let snapshot = client.as_deref().map(Snapshot::read).unwrap_or_default();
        Self {
            app,
            client,
            user,
            snapshot,
            avatar: None,
            avatar_url: None,
            signing_in: None,
            checking: false,
            on_message: None,
            _poll: None,
        }
    }

    pub(crate) fn start(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() {
            return;
        }
        let user = self.user.clone();
        self.set_user(user, cx);
        self.restore(cx);
        self._poll = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                if this.update(cx, |this, cx| this.poll(cx)).is_err() {
                    break;
                }
            }
        }));
    }

    #[must_use]
    pub fn app(&self) -> AccountApp {
        self.app
    }

    /// Whether the account is usable here (there is a home directory).
    #[must_use]
    pub fn available(&self) -> bool {
        self.client.is_some()
    }

    #[must_use]
    pub fn user(&self) -> Option<&Me> {
        self.user.as_ref()
    }

    #[must_use]
    pub fn avatar(&self) -> Option<Arc<Image>> {
        self.avatar.clone()
    }

    /// The launcher's user (for "Use launcher account (@name)").
    #[must_use]
    pub fn launcher_user(&self) -> Option<&Me> {
        self.snapshot.launcher_user.as_ref()
    }

    #[must_use]
    pub fn signing_in(&self) -> bool {
        self.signing_in.is_some()
    }

    /// Restoring the session or signing in from the launcher.
    #[must_use]
    pub fn checking(&self) -> bool {
        self.checking
    }

    #[must_use]
    pub fn status(&self) -> Status {
        Status {
            app: self.app,
            mode: self.snapshot.mode,
            signed_in: self.user.is_some(),
            launcher_signed_in: self.snapshot.launcher_user.is_some(),
        }
    }

    /// The account page on the web.
    #[must_use]
    pub fn account_url(&self) -> Option<String> {
        self.client.as_ref().map(|a| a.account_url())
    }

    pub(crate) fn set_message_handler(&mut self, handler: MessageHandler) {
        self.on_message = Some(handler);
    }

    fn message(&self, text: impl Into<SharedString>, error: bool, cx: &mut App) {
        let text = text.into();
        if error {
            tracing::warn!(message = %text, "Ferrum account");
        }
        match self.on_message.clone() {
            Some(handler) => handler(text, error, cx),
            None => ferrum_ui::notify::notify_in_app("account", text, "", cx),
        }
    }

    /// Checks the session with the server: signs in from the launcher when
    /// following it without a session yet. Offline keeps the cached user.
    pub fn restore(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.client.clone() else {
            return;
        };
        self.checking = true;
        cx.spawn(async move |this, cx| {
            let restored = runtime::run(async move {
                let user = account.restore().await;
                (user, Snapshot::read(&account))
            })
            .await;
            this.update(cx, |this, cx| {
                this.checking = false;
                match restored {
                    Ok((user, snapshot)) => {
                        this.snapshot = snapshot;
                        match user {
                            Ok(user) => this.set_user(user, cx),
                            Err(e) => {
                                tracing::info!(error = %e, "could not check the Ferrum account");
                            }
                        }
                    }
                    Err(e) => tracing::warn!(error = %e, "could not check the Ferrum account"),
                }
                this.changed(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Picks up changes other apps made to `account.json` (the launcher
    /// signing in or out, sign-out everywhere).
    fn poll(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.client.clone() else {
            return;
        };
        if self.checking || self.signing_in.is_some() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let read = runtime::run(async move { Snapshot::read(&account) }).await;
            this.update(cx, |this, cx| {
                let Ok(snapshot) = read else { return };
                if snapshot.key() != this.snapshot.key() && !this.checking {
                    this.snapshot = snapshot;
                    this.restore(cx);
                } else if snapshot != this.snapshot {
                    this.snapshot = snapshot;
                    this.changed(cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the browser to sign in (or switch account) and waits for it.
    pub fn sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.client.clone() else {
            return;
        };
        if self.signing_in.is_some() {
            return;
        }
        let cancel = CancellationToken::new();
        self.signing_in = Some(cancel.clone());
        self.changed(cx);
        cx.spawn(async move |this, cx| {
            let result = runtime::run(async move {
                let result = tokio::select! {
                    result = account.sign_in() => Some(result),
                    () = cancel.cancelled() => None,
                };
                (result, Snapshot::read(&account))
            })
            .await;
            this.update(cx, |this, cx| {
                this.signing_in = None;
                match result {
                    Ok((result, snapshot)) => {
                        this.snapshot = snapshot;
                        match result {
                            Some(Ok(user)) => {
                                let text = format!("Signed in as @{}", user.username);
                                this.set_user(Some(user), cx);
                                this.message(text, false, cx);
                            }
                            Some(Err(AccountError::Denied)) => {
                                this.message("Sign-in cancelled", false, cx);
                            }
                            Some(Err(e)) => {
                                this.message(format!("Could not sign in: {e}"), true, cx)
                            }
                            None => {}
                        }
                    }
                    Err(e) => this.message(format!("Could not sign in: {e}"), true, cx),
                }
                this.changed(cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.signing_in.take() {
            cancel.cancel();
            self.changed(cx);
        }
    }

    /// "Use launcher account": a session of this app's own made from the
    /// launcher's (single sign-on); the app follows the launcher again.
    pub fn use_launcher_account(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.client.clone() else {
            return;
        };
        if self.app.is_launcher() || self.checking {
            return;
        }
        self.checking = true;
        self.changed(cx);
        cx.spawn(async move |this, cx| {
            let result = runtime::run(async move {
                let result = account.sign_in_from_launcher().await;
                (result, Snapshot::read(&account))
            })
            .await;
            this.update(cx, |this, cx| {
                this.checking = false;
                match result {
                    Ok((Ok(user), snapshot)) => {
                        this.snapshot = snapshot;
                        let text = format!("Using the launcher's account, @{}", user.username);
                        this.set_user(Some(user), cx);
                        this.message(text, false, cx);
                    }
                    Ok((Err(e), snapshot)) => {
                        this.snapshot = snapshot;
                        this.message(
                            format!("Could not use the launcher's account: {e}"),
                            true,
                            cx,
                        );
                    }
                    Err(e) => {
                        this.message(
                            format!("Could not use the launcher's account: {e}"),
                            true,
                            cx,
                        );
                    }
                }
                this.changed(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Signs out; the button turns back into "Sign in" right away. Out of
    /// this app only, an app that followed the launcher stays signed out
    /// (others stay signed in).
    pub fn sign_out(&mut self, scope: Scope, cx: &mut Context<Self>) {
        let Some(account) = self.client.clone() else {
            return;
        };
        self.set_user(None, cx);
        let name = ferrum_ui::app_info(cx).name;
        cx.spawn(async move |this, cx| {
            let result = runtime::run(async move {
                let result = account.sign_out(scope).await;
                (result, Snapshot::read(&account))
            })
            .await;
            this.update(cx, |this, cx| {
                match result {
                    Ok((Ok(()), snapshot)) => {
                        this.snapshot = snapshot;
                        this.message(
                            match scope {
                                Scope::ThisApp => format!("Signed out of {name}"),
                                Scope::Everywhere => "Signed out of every Ferrum app".to_owned(),
                            },
                            false,
                            cx,
                        );
                    }
                    Ok((Err(e), snapshot)) => {
                        this.snapshot = snapshot;
                        this.message(format!("Could not sign out: {e}"), true, cx);
                    }
                    Err(e) => this.message(format!("Could not sign out: {e}"), true, cx),
                }
                this.changed(cx);
            })
            .ok();
        })
        .detach();
    }

    fn changed(&self, cx: &mut Context<Self>) {
        cx.notify();
        // The button is drawn by the app's views and the Settings window.
        cx.refresh_windows();
    }

    fn set_user(&mut self, user: Option<Me>, cx: &mut Context<Self>) {
        let url = user.as_ref().and_then(|u| u.avatar_url.clone());
        self.user = user;
        if url != self.avatar_url {
            self.avatar_url = url.clone();
            self.avatar = None;
            if let Some(url) = url {
                self.load_avatar(url, cx);
            }
        }
        self.changed(cx);
    }

    /// Downloads the avatar; until then (or on failure) initials show.
    fn load_avatar(&mut self, url: String, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let fetch_url = url.clone();
            let bytes =
                runtime::run(
                    async move { ferrum_account::fetch_avatar(&fetch_url, AVATAR_PX).await },
                )
                .await;
            let image = match bytes {
                Ok(Ok(bytes)) => {
                    image_format(&bytes).map(|format| Arc::new(Image::from_bytes(format, bytes)))
                }
                Ok(Err(e)) => {
                    tracing::debug!(error = %e, "could not download the avatar");
                    None
                }
                Err(e) => {
                    tracing::debug!(error = %e, "could not download the avatar");
                    None
                }
            };
            this.update(cx, |this, cx| {
                // The user may have changed while it downloaded.
                if this.avatar_url.as_deref() == Some(url.as_str()) {
                    this.avatar = image;
                    this.changed(cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

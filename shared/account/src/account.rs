//! [`Account`]: one app's view of the Ferrum account: the cached user, the
//! in-memory access token (refreshed a minute before it expires), the
//! refresh token in the keychain, and sign-out. Signing in is in
//! [`crate::sign_in`].

use std::path::PathBuf;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Instant;

use crate::api::{Api, Me, TokenResponse};
use crate::secrets::{Keychain, SecretStore};
use crate::store::{AccountFile, FileStore, ferrum_dir, server_url};
use crate::token::AccessGrant;
use crate::{AccessToken, AccountError, App, AppMode, ModeEvent, RefreshToken, Result};

/// What [`Account::sign_out`] ends.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// This app's session only. For the launcher, the server also ends the
    /// app sessions made from it (they are its children).
    ThisApp,
    /// This app and the launcher, and every app on this device goes back
    /// to following the (now signed-out) launcher.
    Everywhere,
}

/// Where an [`Account`] keeps things; [`Account::load`] uses the defaults.
pub struct Options {
    pub app: App,
    /// Base URL, no trailing `/`.
    pub server: String,
    /// Directory of `account.json` (`~/.ferrum`).
    pub dir: PathBuf,
    pub store: Arc<dyn SecretStore>,
}

/// One app's account state. Share it as `Arc<Account>`; the methods take
/// `&self`. Async methods expect a Tokio runtime (keychain calls run on its
/// blocking pool).
pub struct Account {
    pub(crate) app: App,
    pub(crate) api: Api,
    pub(crate) file: FileStore,
    store: Arc<dyn SecretStore>,
    /// Held across a refresh so two callers never rotate the same token.
    grant: tokio::sync::Mutex<Option<AccessGrant>>,
    me: Mutex<Option<Me>>,
}

impl Account {
    /// The app's account with the OS keychain, `~/.ferrum` (or
    /// `$FERRUM_HOME`) and the server from `FERRUM_ACCOUNT_SERVER`.
    pub fn load(app: App) -> Result<Self> {
        let dir = ferrum_dir()
            .ok_or_else(|| AccountError::Invalid("no home directory for ~/.ferrum".into()))?;
        Self::open(Options {
            app,
            server: server_url(|key| std::env::var(key).ok()),
            dir,
            store: Arc::new(Keychain),
        })
    }

    pub fn open(options: Options) -> Result<Self> {
        let file = FileStore::new(&options.dir);
        let cached = file.read()?;
        let me = Self::shares_launcher_user(options.app, &cached)
            .then(|| cached.user.clone())
            .flatten();
        Ok(Self {
            app: options.app,
            api: Api::new(options.server)?,
            file,
            store: options.store,
            grant: tokio::sync::Mutex::new(None),
            me: Mutex::new(me),
        })
    }

    /// Whether this app shows the launcher's cached user.
    fn shares_launcher_user(app: App, file: &AccountFile) -> bool {
        app.is_launcher() || file.mode(app).follows_launcher()
    }

    #[must_use]
    pub fn app(&self) -> App {
        self.app
    }

    #[must_use]
    pub fn server(&self) -> &str {
        self.api.server()
    }

    /// The signed-in user as last seen (from `account.json` at start).
    #[must_use]
    pub fn current_user(&self) -> Option<Me> {
        self.me_slot().clone()
    }

    /// The web page for managing the account.
    #[must_use]
    pub fn account_url(&self) -> String {
        self.current_user()
            .map(|me| me.account_url)
            .filter(|url| !url.is_empty())
            .unwrap_or_else(|| format!("{}/account", self.server()))
    }

    /// This app's mode (always [`AppMode::FollowLauncher`] for the launcher).
    pub fn mode(&self) -> Result<AppMode> {
        Ok(self.file.read()?.mode(self.app))
    }

    pub fn set_mode(&self, mode: AppMode) -> Result<()> {
        if self.app.is_launcher() {
            return Ok(());
        }
        self.file.update(|file| file.set_mode(self.app, mode))
    }

    fn me_slot(&self) -> MutexGuard<'_, Option<Me>> {
        self.me.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// At start: checks the session with the server and returns the user,
    /// or `None` when signed out. An app that follows the launcher signs in
    /// from the launcher's session, and signs out when the launcher has.
    /// Network failures are errors; [`Self::current_user`] keeps the cache.
    pub async fn restore(&self) -> Result<Option<Me>> {
        if !self.app.is_launcher() {
            let file = self.file.read()?;
            match file.mode(self.app) {
                AppMode::SignedOut => return Ok(None),
                AppMode::FollowLauncher if file.user.is_none() => {
                    self.end_session().await;
                    return Ok(None);
                }
                AppMode::FollowLauncher if self.stored_token(self.app).await?.is_none() => {
                    return match self.sign_in_from_launcher().await {
                        Ok(me) => Ok(Some(me)),
                        Err(e) if e.session_ended() => Ok(None),
                        Err(e) => Err(e),
                    };
                }
                AppMode::FollowLauncher | AppMode::OwnAccount => {}
            }
        }
        match self.refresh_user().await {
            Ok(me) => Ok(Some(me)),
            Err(e) if e.session_ended() => {
                self.forget().await;
                Ok(None)
            }
            Err(e) => Err(e),
        }
    }

    /// A valid access token, refreshing (and rotating the refresh token)
    /// when the current one expires within a minute.
    pub async fn access_token(&self) -> Result<AccessToken> {
        let mut grant = self.grant.lock().await;
        if let Some(current) = grant.as_ref()
            && !current.needs_refresh(Instant::now())
        {
            return Ok(current.token.clone());
        }
        let refresh = self
            .stored_token(self.app)
            .await?
            .ok_or(AccountError::SignedOut)?;
        let response = match self.api.refresh(&refresh).await {
            Ok(response) => response,
            Err(e) => {
                if e.session_ended() {
                    *grant = None;
                    drop(grant);
                    self.forget().await;
                }
                return Err(e);
            }
        };
        self.save_token(response.refresh_token.clone()).await?;
        let token = response.access_token.clone();
        *grant = Some(AccessGrant::new(
            response.access_token,
            response.expires_in,
            Instant::now(),
        ));
        drop(grant);
        if let Some(user) = response.user {
            self.remember_user(user)?;
        }
        Ok(token)
    }

    /// Fetches `/api/v1/me` and updates the cache.
    pub async fn refresh_user(&self) -> Result<Me> {
        let token = self.access_token().await?;
        let me = match self.api.me(&token).await {
            // Revoked or expired early: one fresh token, then give up.
            Err(AccountError::Server { status: 401, .. }) => {
                *self.grant.lock().await = None;
                let token = self.access_token().await?;
                self.api.me(&token).await?
            }
            other => other?,
        };
        self.remember_user(me.clone())?;
        Ok(me)
    }

    /// Signs out (see [`Scope`]). The local session always ends; a server
    /// that cannot be reached is logged, not an error.
    pub async fn sign_out(&self, scope: Scope) -> Result<()> {
        self.end_session().await;
        match scope {
            Scope::ThisApp => {
                if self.app.is_launcher() {
                    self.file.update(|file| file.user = None)?;
                } else {
                    self.update_mode(ModeEvent::SignedOutHere)?;
                }
            }
            Scope::Everywhere => {
                if !self.app.is_launcher()
                    && let Some(launcher) = self.stored_token(App::Launcher).await?
                {
                    self.revoke_quietly(&launcher).await;
                    self.keychain(|s| s.delete(App::Launcher)).await?;
                }
                self.file.update(|file| {
                    file.user = None;
                    for app in App::ALL.into_iter().filter(|a| !a.is_launcher()) {
                        let mode = file.mode(app).after(ModeEvent::SignedOutEverywhere);
                        file.set_mode(app, mode);
                    }
                })?;
            }
        }
        Ok(())
    }

    /// Revokes this app's token (best effort) and forgets the session.
    async fn end_session(&self) {
        if let Ok(Some(token)) = self.stored_token(self.app).await {
            self.revoke_quietly(&token).await;
        }
        self.forget().await;
    }

    async fn revoke_quietly(&self, token: &RefreshToken) {
        if let Err(e) = self.api.revoke(token).await {
            tracing::warn!(app = %self.app, error = %e, "could not revoke the session on the server");
        }
    }

    /// Drops the local session: keychain entry, access token, cached user.
    pub(crate) async fn forget(&self) {
        *self.grant.lock().await = None;
        let app = self.app;
        if let Err(e) = self.keychain(move |s| s.delete(app)).await {
            tracing::warn!(app = %self.app, error = %e, "could not remove the refresh token");
        }
        *self.me_slot() = None;
    }

    /// Stores a new session: refresh token, access token, user and mode.
    pub(crate) async fn adopt(&self, response: TokenResponse, event: ModeEvent) -> Result<Me> {
        let previous = self.stored_token(self.app).await?;
        self.save_token(response.refresh_token.clone()).await?;
        if let Some(previous) = previous.filter(|p| *p != response.refresh_token) {
            self.revoke_quietly(&previous).await;
        }
        let access = response.access_token.clone();
        *self.grant.lock().await = Some(AccessGrant::new(
            response.access_token,
            response.expires_in,
            Instant::now(),
        ));
        self.update_mode(event)?;
        let me = match response.user {
            Some(me) => me,
            None => self.api.me(&access).await?,
        };
        self.remember_user(me.clone())?;
        Ok(me)
    }

    fn update_mode(&self, event: ModeEvent) -> Result<()> {
        if self.app.is_launcher() {
            return Ok(());
        }
        self.file.update(|file| {
            let mode = file.mode(self.app).after(event);
            file.set_mode(self.app, mode);
        })
    }

    fn remember_user(&self, me: Me) -> Result<()> {
        *self.me_slot() = Some(me.clone());
        let file = self.file.read()?;
        if Self::shares_launcher_user(self.app, &file) && file.user.as_ref() != Some(&me) {
            self.file.update(|file| file.user = Some(me))?;
        }
        Ok(())
    }

    pub(crate) async fn stored_token(&self, app: App) -> Result<Option<RefreshToken>> {
        self.keychain(move |s| s.get(app)).await
    }

    async fn save_token(&self, token: RefreshToken) -> Result<()> {
        let app = self.app;
        self.keychain(move |s| s.set(app, &token)).await
    }

    /// Runs a (blocking) keychain call on the runtime's blocking pool.
    async fn keychain<T: Send + 'static>(
        &self,
        work: impl FnOnce(&dyn SecretStore) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let store = self.store.clone();
        tokio::task::spawn_blocking(move || work(store.as_ref()))
            .await
            .map_err(|e| AccountError::Keychain(e.to_string()))?
    }
}

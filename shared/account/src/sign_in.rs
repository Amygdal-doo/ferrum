//! Signing in: through the browser (authorization code + PKCE, loopback
//! redirect) or, for an app, from the launcher's session (`exchange`).

use std::time::Duration;

use crate::api::{AuthorizeRequest, Me, authorize_url};
use crate::loopback::Loopback;
use crate::pkce::{Pkce, random_b64};
use crate::{Account, AccountError, App, ModeEvent, Result};

/// How long [`Account::sign_in`] waits for the browser.
pub const SIGN_IN_TIMEOUT: Duration = Duration::from_secs(10 * 60);

/// A browser sign-in in progress: the listener is bound and the URL built.
pub struct SignIn {
    loopback: Loopback,
    verifier: String,
    state: String,
    redirect_uri: String,
    authorize_url: String,
    device_id: String,
}

impl SignIn {
    /// The page to open in the browser.
    #[must_use]
    pub fn authorize_url(&self) -> &str {
        &self.authorize_url
    }

    #[must_use]
    pub fn redirect_uri(&self) -> &str {
        &self.redirect_uri
    }
}

impl Account {
    /// Binds the loopback listener and builds the authorize URL; nothing
    /// is opened yet. Finish with [`Self::finish_sign_in`].
    pub async fn begin_sign_in(&self) -> Result<SignIn> {
        let server = self.server().to_owned();
        let device_id = self
            .file
            .update(|file| {
                file.server = Some(server);
                file.device_id()
            })?
            .to_string();
        let loopback = Loopback::bind().await?;
        let redirect_uri = loopback.redirect_uri();
        let pkce = Pkce::generate()?;
        let state = random_b64(16)?;
        let device_name = crate::system::device_name().await;
        let authorize_url = authorize_url(
            self.server(),
            &AuthorizeRequest {
                app: self.app,
                redirect_uri: &redirect_uri,
                code_challenge: &pkce.challenge,
                state: &state,
                device_name: &device_name,
            },
        )?;
        Ok(SignIn {
            loopback,
            verifier: pkce.verifier,
            state,
            redirect_uri,
            authorize_url,
            device_id,
        })
    }

    /// Waits up to `timeout` for the browser, then trades the code for
    /// tokens. An app signed in this way gets its own account
    /// ([`crate::AppMode::OwnAccount`]).
    pub async fn finish_sign_in(&self, sign_in: SignIn, timeout: Duration) -> Result<Me> {
        let SignIn {
            loopback,
            verifier,
            state,
            redirect_uri,
            device_id,
            ..
        } = sign_in;
        let code = tokio::time::timeout(timeout, loopback.wait(&state))
            .await
            .map_err(|_| AccountError::TimedOut)??;
        let response = self
            .api
            .exchange_code(self.app, &code, &verifier, &redirect_uri, &device_id)
            .await?;
        self.adopt(response, ModeEvent::SignedInInBrowser).await
    }

    /// Opens the browser and waits for the sign-in to finish
    /// ([`SIGN_IN_TIMEOUT`]). Drop the future to cancel.
    pub async fn sign_in(&self) -> Result<Me> {
        let sign_in = self.begin_sign_in().await?;
        crate::system::open_browser(sign_in.authorize_url())?;
        self.finish_sign_in(sign_in, SIGN_IN_TIMEOUT).await
    }

    /// Signs this app in as the launcher's user, with a session of its own
    /// that is a child of the launcher's (single sign-on). Also what "Use
    /// launcher account" does.
    pub async fn sign_in_from_launcher(&self) -> Result<Me> {
        if self.app.is_launcher() {
            return Err(AccountError::Invalid(
                "the launcher signs in through the browser".into(),
            ));
        }
        let parent = self
            .stored_token(App::Launcher)
            .await?
            .ok_or(AccountError::SignedOut)?;
        let server = self.server().to_owned();
        let device_id = self
            .file
            .update(|file| {
                file.server = Some(server);
                file.device_id()
            })?
            .to_string();
        let response = self.api.exchange(&parent, self.app, &device_id).await?;
        self.adopt(response, ModeEvent::UsedLauncherAccount).await
    }
}

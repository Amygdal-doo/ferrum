//! Ferrum account sign-in for the desktop apps (accounts contract v1).
//!
//! - **Sign-in**: OAuth 2.0 authorization code with PKCE (S256). The app
//!   listens on `127.0.0.1:<random port>`, opens the browser at
//!   `{server}/oauth/authorize`, receives `/callback?code=…&state=…` and
//!   trades the code at `/api/v1/oauth/token`.
//! - **Single sign-on**: an app trades the launcher's refresh token for a
//!   child session of its own (`exchange` grant), per [`AppMode`].
//! - **Storage**: refresh tokens in the OS keychain (service
//!   `dev.ferrum.account`, account = app id); access tokens in memory only,
//!   refreshed a minute before they expire; `~/.ferrum/account.json` holds
//!   the device id, the user cache and each app's mode (no secrets).
//! - Tokens are newtypes whose `Debug` is redacted; nothing here logs them.
//!
//! ```no_run
//! # async fn demo() -> ferrum_account::Result<()> {
//! use ferrum_account::{Account, App, Scope};
//! let account = Account::load(App::Launcher)?;
//! let me = account.sign_in().await?; // opens the browser
//! let token = account.access_token().await?; // `Authorization: Bearer …`
//! account.sign_out(Scope::ThisApp).await?;
//! # let _ = (me, token); Ok(()) }
//! ```

mod account;
pub mod api;
mod app;
mod error;
pub mod loopback;
pub mod pkce;
pub mod secrets;
mod sign_in;
pub mod store;
mod system;
mod token;

pub use account::{Account, Options, Scope};
pub use api::{Me, Organization, fetch_avatar};
pub use app::{App, AppMode, ModeEvent};
pub use error::{AccountError, Result};
pub use secrets::{Keychain, MemoryStore, SecretStore};
pub use sign_in::{SIGN_IN_TIMEOUT, SignIn};
pub use system::{device_name, open_browser};
pub use token::{AccessGrant, AccessToken, AuthCode, REFRESH_MARGIN, RefreshToken};

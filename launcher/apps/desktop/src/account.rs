//! The Ferrum account in the launcher (`ferrum_account`): restoring the
//! session at start, browser sign-in (cancellable), sign-out and the
//! avatar image. Keychain and network work runs on the services runtime,
//! never on the UI thread. Drawn by `components::account`.

use crate::app::FerrumLauncher;
use ferrum_account::{Account, AccountError, App as AccountApp, Me, Scope};
use ferrum_ui::gpui::{Image, ImageFormat};
use ferrum_ui::prelude::*;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Avatar download size (the button is 20 px; 2× for Retina, plus room).
const AVATAR_PX: u32 = 64;

/// What the title row and Settings › Account show.
#[derive(Default)]
pub struct AccountUi {
    /// `None` when there is no home directory (the button is hidden).
    pub client: Option<Arc<Account>>,
    pub user: Option<Me>,
    pub avatar: Option<Arc<Image>>,
    avatar_url: Option<String>,
    /// Set while the browser sign-in runs; cancelling stops the wait.
    pub signing_in: Option<CancellationToken>,
}

impl FerrumLauncher {
    /// Loads `account.json` (shows the cached user right away), then checks
    /// the session with the server in the background.
    pub(crate) fn start_account(&mut self, cx: &mut Context<Self>) {
        let account = match Account::load(AccountApp::Launcher) {
            Ok(account) => Arc::new(account),
            Err(e) => {
                tracing::warn!(error = %e, "Ferrum account unavailable");
                return;
            }
        };
        let cached = account.current_user();
        self.account.client = Some(account.clone());
        self.set_user(cached, cx);
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let restored = services.run(async move { account.restore().await }).await;
            this.update(cx, |this, cx| match restored {
                Ok(Ok(user)) => this.set_user(user, cx),
                // Offline: keep the cached user.
                Ok(Err(e)) => tracing::info!(error = %e, "could not check the Ferrum account"),
                Err(e) => tracing::warn!(error = %e, "could not check the Ferrum account"),
            })
            .ok();
        })
        .detach();
    }

    /// Opens the browser to sign in and waits for it to come back.
    pub(crate) fn sign_in(&mut self, cx: &mut Context<Self>) {
        let Some(account) = self.account.client.clone() else {
            return;
        };
        if self.account.signing_in.is_some() {
            return;
        }
        let cancel = CancellationToken::new();
        self.account.signing_in = Some(cancel.clone());
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let result = services
                .run(async move {
                    tokio::select! {
                        result = account.sign_in() => Some(result),
                        () = cancel.cancelled() => None,
                    }
                })
                .await;
            this.update(cx, |this, cx| {
                this.account.signing_in = None;
                match result {
                    Ok(Some(Ok(user))) => {
                        let name = format!("Signed in as @{}", user.username);
                        this.set_user(Some(user), cx);
                        this.notify_user(name, false, cx);
                    }
                    Ok(Some(Err(AccountError::Denied))) => {
                        this.notify_user("Sign-in cancelled", false, cx);
                    }
                    Ok(Some(Err(e))) => {
                        this.notify_user(format!("Could not sign in: {e}"), true, cx);
                    }
                    Ok(None) => {}
                    Err(e) => this.notify_user(format!("Could not sign in: {e}"), true, cx),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn cancel_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.account.signing_in.take() {
            cancel.cancel();
            cx.notify();
        }
    }

    /// Signs out; the button turns back into "Sign in" right away.
    pub(crate) fn sign_out(&mut self, scope: Scope, cx: &mut Context<Self>) {
        let Some(account) = self.account.client.clone() else {
            return;
        };
        self.set_user(None, cx);
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let result = services
                .run(async move { account.sign_out(scope).await })
                .await;
            this.update(cx, |this, cx| match result {
                Ok(Ok(())) => this.notify_user(
                    match scope {
                        Scope::ThisApp => "Signed out of FerrumLauncher",
                        Scope::Everywhere => "Signed out of every Ferrum app",
                    },
                    false,
                    cx,
                ),
                Ok(Err(e)) => this.notify_user(format!("Could not sign out: {e}"), true, cx),
                Err(e) => this.notify_user(format!("Could not sign out: {e}"), true, cx),
            })
            .ok();
        })
        .detach();
    }

    /// The account page on the web (`/account`).
    pub(crate) fn account_url(&self) -> Option<String> {
        self.account.client.as_ref().map(|a| a.account_url())
    }

    fn set_user(&mut self, user: Option<Me>, cx: &mut Context<Self>) {
        let url = user.as_ref().and_then(|u| u.avatar_url.clone());
        self.account.user = user;
        if url != self.account.avatar_url {
            self.account.avatar_url = url.clone();
            self.account.avatar = None;
            if let Some(url) = url {
                self.load_avatar(url, cx);
            }
        }
        cx.notify();
    }

    /// Downloads the avatar; until then (or on failure) initials show.
    fn load_avatar(&mut self, url: String, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let fetch_url = url.clone();
            let bytes = services
                .run(async move { ferrum_account::fetch_avatar(&fetch_url, AVATAR_PX).await })
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
                if this.account.avatar_url.as_deref() == Some(url.as_str()) {
                    this.account.avatar = image;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

/// The image format from its first bytes (GitHub serves PNG or JPEG).
fn image_format(bytes: &[u8]) -> Option<ImageFormat> {
    match bytes {
        [0x89, b'P', b'N', b'G', ..] => Some(ImageFormat::Png),
        [0xff, 0xd8, 0xff, ..] => Some(ImageFormat::Jpeg),
        [b'G', b'I', b'F', b'8', ..] => Some(ImageFormat::Gif),
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => Some(ImageFormat::Webp),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_avatar_formats() {
        assert_eq!(
            image_format(b"\x89PNG\r\n\x1a\n...."),
            Some(ImageFormat::Png)
        );
        assert_eq!(
            image_format(&[0xff, 0xd8, 0xff, 0xe0, 0]),
            Some(ImageFormat::Jpeg)
        );
        assert_eq!(
            image_format(b"RIFF\0\0\0\0WEBPVP8 "),
            Some(ImageFormat::Webp)
        );
        assert_eq!(image_format(b"<html>"), None);
        assert_eq!(image_format(b""), None);
    }
}

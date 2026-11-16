//! End to end against the fake server in `support`: browser sign-in over
//! the loopback, refresh rotation and reuse detection, single sign-on from
//! the launcher, and sign-out.

#![allow(clippy::unwrap_used)]

mod support;

use ferrum_account::store::{FileStore, server_url};
use ferrum_account::{
    Account, AccountError, App, AppMode, MemoryStore, Options, RefreshToken, Scope, SecretStore,
};
use std::sync::Arc;
use std::time::Duration;
use support::{FakeServer, browser};

struct Device {
    dir: tempfile::TempDir,
    store: Arc<MemoryStore>,
}

impl Device {
    fn new() -> Self {
        Self {
            dir: tempfile::tempdir().unwrap(),
            store: Arc::new(MemoryStore::default()),
        }
    }

    fn account(&self, server: &FakeServer, app: App) -> Account {
        Account::open(Options {
            app,
            server: server_url(|_| Some(server.url.clone())),
            dir: self.dir.path().to_owned(),
            store: self.store.clone(),
        })
        .unwrap()
    }

    fn token(&self, app: App) -> Option<RefreshToken> {
        self.store.get(app).unwrap()
    }

    fn file(&self) -> ferrum_account::store::AccountFile {
        FileStore::new(self.dir.path()).read().unwrap()
    }
}

/// Signs `account` in through the (simulated) browser.
async fn sign_in(account: &Account) -> Result<ferrum_account::Me, AccountError> {
    let sign_in = account.begin_sign_in().await?;
    let page = tokio::spawn(browser(sign_in.authorize_url().to_owned()));
    let result = account
        .finish_sign_in(sign_in, Duration::from_secs(10))
        .await;
    let page = page.await.unwrap();
    if result.is_ok() {
        assert!(page.contains("you can close this tab"), "{page}");
    }
    result
}

#[tokio::test]
async fn browser_sign_in_refresh_and_sign_out() {
    let server = FakeServer::start().await;
    let device = Device::new();
    let launcher = device.account(&server, App::Launcher);
    assert!(launcher.current_user().is_none());
    assert!(matches!(
        launcher.access_token().await,
        Err(AccountError::SignedOut)
    ));

    let me = sign_in(&launcher).await.unwrap();
    assert_eq!(me.username, "ada");
    assert_eq!(launcher.current_user().unwrap().username, "ada");
    assert_eq!(launcher.account_url(), "http://fake/account");
    let file = device.file();
    assert_eq!(file.user.as_ref().unwrap().username, "ada");
    assert!(file.device_id.is_some());
    assert_eq!(file.server.as_deref(), Some(server.url.as_str()));
    assert_eq!(server.state().sessions[0].app, "launcher");
    assert_eq!(
        server.state().sessions[0].device_id,
        file.device_id.unwrap().to_string()
    );

    // The access token from the code grant is reused while fresh.
    let first_refresh = device.token(App::Launcher).unwrap();
    launcher.access_token().await.unwrap();
    launcher.refresh_user().await.unwrap();
    assert_eq!(server.state().refresh_grants, 0);
    assert_eq!(device.token(App::Launcher).unwrap(), first_refresh);

    // A new process starts with only the keychain token: it refreshes,
    // and the stored refresh token rotates.
    let restarted = device.account(&server, App::Launcher);
    assert_eq!(restarted.restore().await.unwrap().unwrap().username, "ada");
    assert_eq!(server.state().refresh_grants, 1);
    let second_refresh = device.token(App::Launcher).unwrap();
    assert_ne!(second_refresh, first_refresh);

    restarted.sign_out(Scope::ThisApp).await.unwrap();
    assert!(device.token(App::Launcher).is_none());
    assert!(restarted.current_user().is_none());
    assert!(device.file().user.is_none());
    assert!(server.state().sessions[0].revoked);
}

#[tokio::test]
async fn short_lived_access_tokens_refresh_and_rotate_before_expiry() {
    let server = FakeServer::start().await;
    server.state().expires_in = 30; // inside the one-minute margin
    let device = Device::new();
    let launcher = device.account(&server, App::Launcher);
    sign_in(&launcher).await.unwrap();

    let mut seen = vec![device.token(App::Launcher).unwrap()];
    for _ in 0..3 {
        launcher.access_token().await.unwrap();
        seen.push(device.token(App::Launcher).unwrap());
    }
    assert_eq!(server.state().refresh_grants, 3);
    seen.dedup();
    assert_eq!(seen.len(), 4, "each refresh rotates the token");

    // Presenting an old token again (a stolen copy) ends the session; the
    // client then finds itself signed out and forgets its token.
    let api = ferrum_account::api::Api::new(server.url.clone()).unwrap();
    let reuse = api.refresh(&seen[1]).await.unwrap_err();
    assert!(reuse.session_ended(), "{reuse}");
    let err = launcher.access_token().await.unwrap_err();
    assert!(err.session_ended(), "{err}");
    assert!(device.token(App::Launcher).is_none());
    assert!(launcher.current_user().is_none());
}

#[tokio::test]
async fn apps_follow_the_launcher_and_sign_out_per_app() {
    let server = FakeServer::start().await;
    let device = Device::new();
    let launcher = device.account(&server, App::Launcher);

    // Nothing to follow yet.
    let editor = device.account(&server, App::Editor);
    assert!(editor.restore().await.unwrap().is_none());

    sign_in(&launcher).await.unwrap();
    let editor = device.account(&server, App::Editor);
    assert_eq!(editor.current_user().unwrap().username, "ada", "cached");
    assert_eq!(editor.restore().await.unwrap().unwrap().username, "ada");
    assert_eq!(editor.mode().unwrap(), AppMode::FollowLauncher);
    {
        let state = server.state();
        assert_eq!(state.sessions[1].app, "editor");
        assert_eq!(state.sessions[1].parent, Some(0));
    }
    let editor_token = device.token(App::Editor).unwrap();
    assert_ne!(Some(editor_token), device.token(App::Launcher));

    // Signing out of the editor leaves the launcher signed in, and the
    // editor stays out even though the launcher is signed in.
    editor.sign_out(Scope::ThisApp).await.unwrap();
    assert_eq!(editor.mode().unwrap(), AppMode::SignedOut);
    assert!(device.token(App::Editor).is_none());
    assert!(server.state().sessions[1].revoked);
    assert!(!server.state().sessions[0].revoked);
    assert!(launcher.refresh_user().await.is_ok());
    let editor = device.account(&server, App::Editor);
    assert!(editor.current_user().is_none());
    assert!(editor.restore().await.unwrap().is_none());

    // "Use launcher account" brings it back.
    editor.sign_in_from_launcher().await.unwrap();
    assert_eq!(editor.mode().unwrap(), AppMode::FollowLauncher);

    // The launcher signing out ends the editor's child session too.
    let git = device.account(&server, App::Git);
    git.restore().await.unwrap().unwrap();
    launcher.sign_out(Scope::ThisApp).await.unwrap();
    assert!(server.state().sessions.iter().all(|s| s.revoked));
    let git = device.account(&server, App::Git);
    assert!(git.restore().await.unwrap().is_none());
    assert!(device.token(App::Git).is_none());
}

#[tokio::test]
async fn own_account_and_sign_out_everywhere() {
    let server = FakeServer::start().await;
    let device = Device::new();
    let launcher = device.account(&server, App::Launcher);
    sign_in(&launcher).await.unwrap();

    let term = device.account(&server, App::Term);
    sign_in(&term).await.unwrap();
    assert_eq!(term.mode().unwrap(), AppMode::OwnAccount);
    assert_eq!(server.state().sessions[1].parent, None);

    term.sign_out(Scope::Everywhere).await.unwrap();
    assert!(device.token(App::Term).is_none());
    assert!(device.token(App::Launcher).is_none());
    assert!(server.state().sessions.iter().all(|s| s.revoked));
    let file = device.file();
    assert!(file.user.is_none());
    assert!(
        App::ALL
            .iter()
            .all(|app| file.mode(*app) == AppMode::FollowLauncher)
    );
}

#[tokio::test]
async fn cancel_in_the_browser_and_a_forged_callback() {
    let server = FakeServer::start().await;
    server.state().deny = true;
    let device = Device::new();
    let launcher = device.account(&server, App::Launcher);
    assert!(matches!(
        sign_in(&launcher).await,
        Err(AccountError::Denied)
    ));
    assert!(device.token(App::Launcher).is_none());

    // A callback with another state is refused and nothing is stored.
    let pending = launcher.begin_sign_in().await.unwrap();
    let forged = format!("{}?code=fac_forged&state=nope", pending.redirect_uri());
    let page = tokio::spawn(async move { reqwest::get(forged).await.unwrap().status() });
    let result = launcher
        .finish_sign_in(pending, Duration::from_secs(10))
        .await;
    assert!(matches!(result, Err(AccountError::StateMismatch)));
    assert_eq!(page.await.unwrap().as_u16(), 400);
    assert!(device.token(App::Launcher).is_none());

    // Nobody comes back: the wait times out.
    let pending = launcher.begin_sign_in().await.unwrap();
    assert!(matches!(
        launcher
            .finish_sign_in(pending, Duration::from_millis(50))
            .await,
        Err(AccountError::TimedOut)
    ));
}

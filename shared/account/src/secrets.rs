//! Refresh tokens in the OS keychain: service [`KEYCHAIN_SERVICE`], account
//! = the app id (`launcher`, `editor`, …). Behind [`SecretStore`] so tests
//! use [`MemoryStore`]. Keychain calls block (and may show a prompt): call
//! them off the UI thread.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::{AccountError, App, RefreshToken, Result};

pub const KEYCHAIN_SERVICE: &str = "dev.ferrum.account";

/// Where refresh tokens are kept, one per app.
pub trait SecretStore: Send + Sync {
    fn get(&self, app: App) -> Result<Option<RefreshToken>>;
    fn set(&self, app: App, token: &RefreshToken) -> Result<()>;
    /// Removes the token; a missing one is fine.
    fn delete(&self, app: App) -> Result<()>;
}

/// The OS keychain (macOS Keychain, Windows Credential Manager, Secret Service).
pub struct Keychain;

fn entry(app: App) -> Result<keyring::Entry> {
    keyring::Entry::new(KEYCHAIN_SERVICE, app.id())
        .map_err(|e| AccountError::Keychain(e.to_string()))
}

impl SecretStore for Keychain {
    fn get(&self, app: App) -> Result<Option<RefreshToken>> {
        match entry(app)?.get_password() {
            Ok(token) if !token.trim().is_empty() => Ok(Some(RefreshToken::new(token.trim()))),
            Ok(_) | Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(AccountError::Keychain(e.to_string())),
        }
    }

    fn set(&self, app: App, token: &RefreshToken) -> Result<()> {
        entry(app)?
            .set_password(token.expose())
            .map_err(|e| AccountError::Keychain(e.to_string()))
    }

    fn delete(&self, app: App) -> Result<()> {
        match entry(app)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(AccountError::Keychain(e.to_string())),
        }
    }
}

/// An in-memory store for tests (and for apps run with no keychain).
#[derive(Default)]
pub struct MemoryStore(Mutex<HashMap<App, RefreshToken>>);

impl MemoryStore {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<App, RefreshToken>> {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

impl SecretStore for MemoryStore {
    fn get(&self, app: App) -> Result<Option<RefreshToken>> {
        Ok(self.map().get(&app).cloned())
    }

    fn set(&self, app: App, token: &RefreshToken) -> Result<()> {
        self.map().insert(app, token.clone());
        Ok(())
    }

    fn delete(&self, app: App) -> Result<()> {
        self.map().remove(&app);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_keeps_one_token_per_app() {
        let store = MemoryStore::default();
        assert!(store.get(App::Launcher).expect("get").is_none());
        store
            .set(App::Launcher, &RefreshToken::new("frt_a"))
            .expect("set");
        store
            .set(App::Editor, &RefreshToken::new("frt_b"))
            .expect("set");
        assert_eq!(
            store.get(App::Launcher).expect("get"),
            Some(RefreshToken::new("frt_a"))
        );
        store.delete(App::Launcher).expect("delete");
        store.delete(App::Launcher).expect("delete again");
        assert!(store.get(App::Launcher).expect("get").is_none());
        assert!(store.get(App::Editor).expect("get").is_some());
    }
}

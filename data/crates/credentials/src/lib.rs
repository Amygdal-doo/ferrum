//! Credential storage.
//!
//! Secrets live in the OS vault (macOS Keychain, Windows Credential Manager,
//! Linux Secret Service) via `keyring`. When the vault is unavailable the app
//! falls back to [`SessionStore`], which keeps secrets in memory only and never
//! writes them to disk (ADR-007).

use ferrumdata_domain::{Secret, SecretRef};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("the OS credential store is unavailable: {0}")]
    Unavailable(String),
    #[error("credential store error: {0}")]
    Store(String),
}

pub type CredentialResult<T> = Result<T, CredentialError>;

/// Blocking API: OS vault calls may block on user prompts, so callers run
/// them on a blocking thread, never on the UI thread.
pub trait CredentialStore: Send + Sync {
    fn name(&self) -> &'static str;
    fn get(&self, key: &SecretRef) -> CredentialResult<Option<Secret>>;
    fn set(&self, key: &SecretRef, secret: &Secret) -> CredentialResult<()>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, key: &SecretRef) -> CredentialResult<()>;
}

/// OS-native vault.
#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    /// Returns the store if the platform vault initialised successfully.
    pub fn try_new() -> CredentialResult<Self> {
        match keyring::Entry::store_status() {
            Ok(()) => Ok(Self),
            Err(e) => Err(CredentialError::Unavailable(e.to_string())),
        }
    }

    fn entry(key: &SecretRef) -> CredentialResult<keyring::Entry> {
        keyring::Entry::new(&key.service, &key.account).map_err(map_err)
    }
}

fn map_err(e: keyring::Error) -> CredentialError {
    match e {
        keyring::Error::NoStorageAccess(_) => CredentialError::Unavailable(e.to_string()),
        other => CredentialError::Store(other.to_string()),
    }
}

impl CredentialStore for KeyringStore {
    fn name(&self) -> &'static str {
        "os-keychain"
    }

    fn get(&self, key: &SecretRef) -> CredentialResult<Option<Secret>> {
        match Self::entry(key)?.get_password() {
            Ok(pw) => Ok(Some(Secret::new(pw))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, key: &SecretRef, secret: &Secret) -> CredentialResult<()> {
        Self::entry(key)?
            .set_password(secret.expose())
            .map_err(map_err)
    }

    fn delete(&self, key: &SecretRef) -> CredentialResult<()> {
        match Self::entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

/// In-memory, process-lifetime store for session-only passwords.
#[derive(Debug, Default, Clone)]
pub struct SessionStore {
    inner: Arc<Mutex<HashMap<SecretRef, Secret>>>,
}

impl SessionStore {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<SecretRef, Secret>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl CredentialStore for SessionStore {
    fn name(&self) -> &'static str {
        "session-memory"
    }

    fn get(&self, key: &SecretRef) -> CredentialResult<Option<Secret>> {
        Ok(self.map().get(key).cloned())
    }

    fn set(&self, key: &SecretRef, secret: &Secret) -> CredentialResult<()> {
        self.map().insert(key.clone(), secret.clone());
        Ok(())
    }

    fn delete(&self, key: &SecretRef) -> CredentialResult<()> {
        self.map().remove(key);
        Ok(())
    }
}

/// Picks the OS vault, falling back to session memory with a warning.
#[must_use]
pub fn default_store() -> Arc<dyn CredentialStore> {
    match KeyringStore::try_new() {
        Ok(store) => Arc::new(store),
        Err(e) => {
            tracing::warn!(error = %e, "OS credential store unavailable; passwords are session-only");
            Arc::new(SessionStore::new())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::ProfileId;

    #[test]
    fn session_store_roundtrip() {
        let store = SessionStore::new();
        let key = SecretRef::for_profile(ProfileId::new());
        assert!(store.get(&key).expect("get").is_none());
        store.set(&key, &Secret::new("pw")).expect("set");
        assert_eq!(store.get(&key).expect("get").expect("some").expose(), "pw");
        store.delete(&key).expect("delete");
        store.delete(&key).expect("delete twice is fine");
        assert!(store.get(&key).expect("get").is_none());
    }

    /// Touches the real OS keychain; run manually: `cargo test -p ferrumdata-credentials -- --ignored`.
    #[test]
    #[ignore = "requires an unlocked OS keychain and may prompt"]
    fn keyring_roundtrip() {
        let store = KeyringStore::try_new().expect("keychain available");
        let key = SecretRef {
            service: "dev.ferrumdata.test".into(),
            account: ProfileId::new().to_string(),
        };
        store.set(&key, &Secret::new("s3cret")).expect("set");
        assert_eq!(
            store.get(&key).expect("get").expect("some").expose(),
            "s3cret"
        );
        store.delete(&key).expect("delete");
        assert!(store.get(&key).expect("get").is_none());
    }
}

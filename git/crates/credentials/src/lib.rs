//! SSH key passphrases in the OS vault (macOS Keychain, Windows Credential
//! Manager, Linux Secret Service), keyed by profile id. Falls back to
//! session memory when the vault is unavailable; never writes to disk.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// Keychain service name for FerrumGIT passphrases.
pub const SERVICE: &str = "dev.ferrumgit.ssh-passphrase";

#[derive(Debug, thiserror::Error)]
pub enum CredentialError {
    #[error("the OS credential store is unavailable: {0}")]
    Unavailable(String),
    #[error("credential store error: {0}")]
    Store(String),
}

pub type CredentialResult<T> = Result<T, CredentialError>;

/// Blocking API: vault calls may show OS prompts, so call them from a
/// blocking thread, never the UI thread.
pub trait CredentialStore: Send + Sync {
    fn name(&self) -> &'static str;
    fn get(&self, account: &str) -> CredentialResult<Option<String>>;
    fn set(&self, account: &str, secret: &str) -> CredentialResult<()>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, account: &str) -> CredentialResult<()>;
}

#[derive(Debug, Default)]
pub struct KeyringStore;

impl KeyringStore {
    pub fn try_new() -> CredentialResult<Self> {
        match keyring::Entry::store_status() {
            Ok(()) => Ok(Self),
            Err(e) => Err(CredentialError::Unavailable(e.to_string())),
        }
    }

    fn entry(account: &str) -> CredentialResult<keyring::Entry> {
        keyring::Entry::new(SERVICE, account).map_err(map_err)
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

    fn get(&self, account: &str) -> CredentialResult<Option<String>> {
        match Self::entry(account)?.get_password() {
            Ok(pw) => Ok(Some(pw)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, account: &str, secret: &str) -> CredentialResult<()> {
        Self::entry(account)?.set_password(secret).map_err(map_err)
    }

    fn delete(&self, account: &str) -> CredentialResult<()> {
        match Self::entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

/// In-memory, process-lifetime store.
#[derive(Debug, Default, Clone)]
pub struct SessionStore {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl SessionStore {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, String>> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

impl CredentialStore for SessionStore {
    fn name(&self) -> &'static str {
        "session-memory"
    }

    fn get(&self, account: &str) -> CredentialResult<Option<String>> {
        Ok(self.map().get(account).cloned())
    }

    fn set(&self, account: &str, secret: &str) -> CredentialResult<()> {
        self.map().insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> CredentialResult<()> {
        self.map().remove(account);
        Ok(())
    }
}

/// The OS vault, or session memory with a warning.
#[must_use]
pub fn default_store() -> Arc<dyn CredentialStore> {
    match KeyringStore::try_new() {
        Ok(s) => Arc::new(s),
        Err(e) => {
            tracing::warn!(error = %e, "OS credential store unavailable; passphrases are session-only");
            Arc::new(SessionStore::default())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_store_round_trip() {
        let s = SessionStore::default();
        assert_eq!(s.get("p").expect("get"), None);
        s.set("p", "pw").expect("set");
        assert_eq!(s.get("p").expect("get").as_deref(), Some("pw"));
        s.delete("p").expect("delete");
        s.delete("p").expect("idempotent");
        assert_eq!(s.get("p").expect("get"), None);
    }

    /// Real OS keychain round trip; may prompt. `cargo test -p ferrumgit-credentials -- --ignored`
    #[test]
    #[ignore = "touches the real OS keychain"]
    fn keychain_round_trip() {
        let s = KeyringStore::try_new().expect("vault");
        let account = "ferrumgit-test-account";
        s.set(account, "secret").expect("set");
        assert_eq!(s.get(account).expect("get").as_deref(), Some("secret"));
        s.delete(account).expect("delete");
    }
}

//! API keys: the OS vault (macOS Keychain, Windows Credential Manager,
//! Linux Secret Service) or an environment variable. Keys are never
//! logged, never written to plain files, and [`ApiKey`] has no `Display`
//! and a redacting `Debug`.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, PoisonError};

/// Keychain service FerrumChat stores provider keys under (account =
/// provider id, e.g. `anthropic`).
pub const SERVICE: &str = "dev.ferrum.agent.api-key";

/// A secret value. Read it with [`ApiKey::expose`] only where it is sent.
#[derive(Clone, PartialEq, Eq)]
pub struct ApiKey(String);

impl ApiKey {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    /// What a UI may show: `sk-a…xyz9` style hint for long keys.
    pub fn hint(&self) -> String {
        let chars: Vec<char> = self.0.chars().collect();
        if chars.len() < 12 {
            "••••••••".into()
        } else {
            let head: String = chars[..4].iter().collect();
            let tail: String = chars[chars.len() - 4..].iter().collect();
            format!("{head}…{tail}")
        }
    }
}

impl std::fmt::Debug for ApiKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ApiKey(<redacted>)")
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SecretError {
    #[error("the OS credential store is unavailable: {0}")]
    Unavailable(String),
    #[error("credential store error: {0}")]
    Store(String),
}

/// Blocking: vault calls may show OS prompts, so call them off the UI
/// thread.
pub trait SecretStore: Send + Sync {
    fn name(&self) -> &'static str;
    fn get(&self, account: &str) -> Result<Option<ApiKey>, SecretError>;
    fn set(&self, account: &str, secret: &ApiKey) -> Result<(), SecretError>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// The OS vault.
#[derive(Debug)]
pub struct KeychainStore {
    service: &'static str,
}

impl KeychainStore {
    pub fn try_new() -> Result<Self, SecretError> {
        if let Err(e) = keyring::Entry::store_status() {
            return Err(SecretError::Unavailable(e.to_string()));
        }
        Ok(Self { service: SERVICE })
    }

    fn entry(&self, account: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(self.service, account).map_err(map_err)
    }
}

fn map_err(e: keyring::Error) -> SecretError {
    match e {
        keyring::Error::NoStorageAccess(_) => SecretError::Unavailable(e.to_string()),
        other => SecretError::Store(other.to_string()),
    }
}

impl SecretStore for KeychainStore {
    fn name(&self) -> &'static str {
        "os-keychain"
    }

    fn get(&self, account: &str) -> Result<Option<ApiKey>, SecretError> {
        match self.entry(account)?.get_password() {
            Ok(value) => Ok(Some(ApiKey(value))),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, account: &str, secret: &ApiKey) -> Result<(), SecretError> {
        self.entry(account)?
            .set_password(secret.expose())
            .map_err(map_err)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match self.entry(account)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(map_err(e)),
        }
    }
}

/// Process-lifetime memory (tests, or when the vault is unavailable).
#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    inner: Arc<Mutex<HashMap<String, String>>>,
}

impl SecretStore for MemoryStore {
    fn name(&self) -> &'static str {
        "session-memory"
    }

    fn get(&self, account: &str) -> Result<Option<ApiKey>, SecretError> {
        let map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(map.get(account).cloned().map(ApiKey))
    }

    fn set(&self, account: &str, secret: &ApiKey) -> Result<(), SecretError> {
        let mut map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        map.insert(account.to_string(), secret.0.clone());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        let mut map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        map.remove(account);
        Ok(())
    }
}

/// The OS vault, or session memory when there is none.
pub fn default_store() -> Arc<dyn SecretStore> {
    match KeychainStore::try_new() {
        Ok(store) => Arc::new(store),
        Err(error) => {
            tracing::warn!("API keys kept in memory only: {error}");
            Arc::new(MemoryStore::default())
        }
    }
}

/// Where a key came from, for the settings sheet.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    Keychain,
    Env,
}

/// The key for `account`: the vault first, then `env_var`.
pub fn resolve(
    store: &dyn SecretStore,
    account: &str,
    env_var: Option<&str>,
) -> Option<(ApiKey, KeySource)> {
    if let Ok(Some(key)) = store.get(account)
        && !key.expose().trim().is_empty()
    {
        return Some((key, KeySource::Keychain));
    }
    let value = std::env::var(env_var?).ok()?;
    (!value.trim().is_empty()).then(|| (ApiKey(value.trim().to_string()), KeySource::Env))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_redacted_and_resolved_store_first() {
        let key = ApiKey::new("sk-ant-0123456789abcdef");
        assert_eq!(format!("{key:?}"), "ApiKey(<redacted>)");
        assert_eq!(key.hint(), "sk-a…cdef");
        assert_eq!(ApiKey::new("short").hint(), "••••••••");
        let store = MemoryStore::default();
        assert!(resolve(&store, "anthropic", Some("FERRUM_TEST_SURELY_UNSET_VAR")).is_none());
        store.set("anthropic", &key).expect("set");
        let (found, source) = resolve(&store, "anthropic", None).expect("found");
        assert_eq!((found, source), (key, KeySource::Keychain));
        store.delete("anthropic").expect("delete");
        store.delete("anthropic").expect("idempotent");
        assert!(store.get("anthropic").expect("get").is_none());
    }
}

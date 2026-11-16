//! Auth secrets and OAuth 2 tokens in the OS vault (macOS Keychain, Windows
//! Credential Manager, Linux Secret Service) through `keyring`, the same
//! mechanism as FerrumData, FerrumGIT and FerrumChat.
//!
//! One entry per auth config (account = its `secret_ref`, value = a JSON
//! object of its secret fields) and one per cached token
//! (`token:{secret_ref}:{environment}`). Collections, tabs, history and
//! exported files only ever hold the `secret_ref`. Values are never logged.

use crate::AppServices;
use ferrumapi_domain::{Auth, Collection, EnvironmentId, Item, OAuth2Token};
use std::collections::{BTreeMap, HashMap};
use std::sync::{Arc, Mutex, PoisonError};

/// Keychain service FerrumAPI stores secrets under.
pub const SERVICE: &str = "dev.ferrum.api.secret";

#[derive(Debug, Clone, thiserror::Error)]
pub enum SecretError {
    #[error("the OS credential store is unavailable: {0}")]
    Unavailable(String),
    #[error("credential store error: {0}")]
    Store(String),
}

/// Blocking: vault calls may show OS prompts, so they run on blocking
/// threads of the services runtime, never on the UI thread.
pub trait SecretStore: Send + Sync {
    fn name(&self) -> &'static str;
    fn get(&self, account: &str) -> Result<Option<String>, SecretError>;
    fn set(&self, account: &str, value: &str) -> Result<(), SecretError>;
    /// Deleting a missing entry is not an error.
    fn delete(&self, account: &str) -> Result<(), SecretError>;
}

/// The OS vault.
#[derive(Debug)]
pub struct KeychainStore;

impl KeychainStore {
    pub fn try_new() -> Result<Self, SecretError> {
        if let Err(e) = keyring::Entry::store_status() {
            return Err(SecretError::Unavailable(e.to_string()));
        }
        Ok(Self)
    }

    fn entry(account: &str) -> Result<keyring::Entry, SecretError> {
        keyring::Entry::new(SERVICE, account).map_err(map_err)
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

    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        match Self::entry(account)?.get_password() {
            Ok(v) => Ok(Some(v)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(map_err(e)),
        }
    }

    fn set(&self, account: &str, value: &str) -> Result<(), SecretError> {
        Self::entry(account)?.set_password(value).map_err(map_err)
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        match Self::entry(account)?.delete_credential() {
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

impl MemoryStore {
    /// Every account and value (tests).
    #[must_use]
    pub fn dump(&self) -> HashMap<String, String> {
        self.inner
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl SecretStore for MemoryStore {
    fn name(&self) -> &'static str {
        "session-memory"
    }

    fn get(&self, account: &str) -> Result<Option<String>, SecretError> {
        let map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(map.get(account).cloned())
    }

    fn set(&self, account: &str, value: &str) -> Result<(), SecretError> {
        let mut map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        map.insert(account.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, account: &str) -> Result<(), SecretError> {
        let mut map = self.inner.lock().unwrap_or_else(PoisonError::into_inner);
        map.remove(account);
        Ok(())
    }
}

/// The OS vault, or session memory when there is none.
#[must_use]
pub fn default_store() -> Arc<dyn SecretStore> {
    match KeychainStore::try_new() {
        Ok(store) => Arc::new(store),
        Err(error) => {
            tracing::warn!("auth secrets kept in memory only: {error}");
            Arc::new(MemoryStore::default())
        }
    }
}

/// Keychain account of the token cached for `secret_ref` in `environment`.
#[must_use]
pub fn token_account(secret_ref: &str, environment: Option<EnvironmentId>) -> String {
    let env = environment.map_or_else(|| "none".to_owned(), |e| e.to_string());
    format!("token:{secret_ref}:{env}")
}

/// A stored entry: a JSON object of field → value.
fn decode(json: &str) -> BTreeMap<String, String> {
    serde_json::from_str(json).unwrap_or_default()
}

/// Every auth in a collection (its own, folders', requests').
fn auths_mut(c: &mut Collection) -> Vec<&mut Auth> {
    fn walk<'a>(items: &'a mut [Item], out: &mut Vec<&'a mut Auth>) {
        for item in items {
            match item {
                Item::Folder(f) => {
                    out.push(&mut f.auth);
                    walk(&mut f.items, out);
                }
                Item::Request(r) => out.push(&mut r.request.auth),
            }
        }
    }
    let mut out = vec![&mut c.auth];
    walk(&mut c.items, &mut out);
    out
}

/// Last value written per account, so autosave writes only on change.
#[derive(Default, Clone)]
pub(crate) struct Written(Arc<Mutex<HashMap<String, String>>>);

impl AppServices {
    /// Merges the non-empty secret fields of `auths` into the vault (writes
    /// only on change). Empty fields never erase stored ones: an auth read
    /// back from disk has empty secrets until it is hydrated. Use
    /// [`AppServices::replace_secrets`] for what the user typed.
    pub async fn stash_secrets(&self, auths: Vec<Auth>) -> Result<(), SecretError> {
        self.write_secrets(auths, false).await
    }

    /// Stores exactly the secret fields of `auths` (the user's edits): a
    /// field cleared in the editor is removed, all cleared deletes the entry.
    pub async fn replace_secrets(&self, auths: Vec<Auth>) -> Result<(), SecretError> {
        self.write_secrets(auths, true).await
    }

    async fn write_secrets(&self, auths: Vec<Auth>, replace: bool) -> Result<(), SecretError> {
        let store = self.secrets.clone();
        let written = self.secrets_written.clone();
        let work = move || {
            for auth in &auths {
                let Some(account) = auth.secret_ref() else {
                    continue;
                };
                let fields = auth
                    .secret_fields()
                    .into_iter()
                    .filter(|(_, v)| !v.is_empty())
                    .map(|(k, v)| (k.to_owned(), v.to_owned()));
                if !replace && fields.clone().next().is_none() {
                    continue;
                }
                let mut seen = written.0.lock().unwrap_or_else(PoisonError::into_inner);
                let current: BTreeMap<String, String> = match seen.get(account) {
                    Some(json) => decode(json),
                    None => store
                        .get(account)?
                        .as_deref()
                        .map(decode)
                        .unwrap_or_default(),
                };
                let mut next = if replace {
                    BTreeMap::new()
                } else {
                    current.clone()
                };
                next.extend(fields);
                let json = serde_json::to_string(&next).unwrap_or_default();
                if next != current {
                    if next.is_empty() {
                        store.delete(account)?;
                    } else {
                        store.set(account, &json)?;
                    }
                }
                seen.insert(account.to_owned(), json);
            }
            Ok(())
        };
        tokio::task::spawn_blocking(work)
            .await
            .map_err(|e| SecretError::Store(e.to_string()))?
    }

    /// [`AppServices::stash_secrets`] for every auth in `collection`.
    pub async fn stash_collection_secrets(
        &self,
        collection: &Collection,
    ) -> Result<(), SecretError> {
        let mut c = collection.clone();
        let auths = auths_mut(&mut c).into_iter().map(|a| a.clone()).collect();
        self.stash_secrets(auths).await
    }

    /// Saves `collection` to SQLite (secrets left out by serde) after moving
    /// its auth secrets to the vault. A vault failure is logged, not fatal.
    pub async fn save_collection(&self, collection: &Collection) -> Result<(), crate::StoreError> {
        if let Err(e) = self.stash_collection_secrets(collection).await {
            tracing::warn!(error = %e, "could not save auth secrets to the keychain");
        }
        self.store.save_collection(collection).await
    }

    /// Fills the empty secret fields of `auths` from the vault. Failures are
    /// logged; the auth is left as it was.
    pub async fn hydrate_secrets(&self, auths: Vec<&mut Auth>) {
        let accounts: Vec<String> = auths
            .iter()
            .filter(|a| a.secrets_empty())
            .filter_map(|a| a.secret_ref().map(str::to_owned))
            .collect();
        if accounts.is_empty() {
            return;
        }
        let store = self.secrets.clone();
        let written = self.secrets_written.clone();
        let found = tokio::task::spawn_blocking(move || {
            let mut found: HashMap<String, String> = HashMap::new();
            for account in accounts {
                match store.get(&account) {
                    Ok(Some(v)) => {
                        written
                            .0
                            .lock()
                            .unwrap_or_else(PoisonError::into_inner)
                            .insert(account.clone(), v.clone());
                        found.insert(account, v);
                    }
                    Ok(None) => {}
                    Err(e) => tracing::warn!(error = %e, "could not read an auth secret"),
                }
            }
            found
        })
        .await
        .unwrap_or_default();
        for auth in auths {
            let Some(json) = auth.secret_ref().and_then(|r| found.get(r)) else {
                continue;
            };
            if auth.secrets_empty() {
                for (k, v) in decode(json) {
                    auth.set_secret_field(&k, v);
                }
            }
        }
    }

    /// [`AppServices::hydrate_secrets`] for every auth in `collections`.
    pub async fn hydrate_collections(&self, collections: &mut [Collection]) {
        let auths = collections.iter_mut().flat_map(auths_mut).collect();
        self.hydrate_secrets(auths).await;
    }

    /// The cached OAuth 2 token of `secret_ref` in `environment`, from
    /// memory or the vault.
    pub async fn cached_token(
        &self,
        secret_ref: &str,
        environment: Option<EnvironmentId>,
    ) -> Option<OAuth2Token> {
        let account = token_account(secret_ref, environment);
        if let Some(t) = self.tokens().get(&account) {
            return Some(t.clone());
        }
        let store = self.secrets.clone();
        let key = account.clone();
        let json = tokio::task::spawn_blocking(move || store.get(&key))
            .await
            .ok()?
            .ok()??;
        let token: OAuth2Token = serde_json::from_str(&json).ok()?;
        self.tokens().insert(account, token.clone());
        Some(token)
    }

    /// Caches `token` (memory and vault); `None` forgets it.
    pub async fn store_token(
        &self,
        secret_ref: &str,
        environment: Option<EnvironmentId>,
        token: Option<OAuth2Token>,
    ) -> Result<(), SecretError> {
        let account = token_account(secret_ref, environment);
        let json = token.as_ref().map(serde_json::to_string).transpose();
        let json = json.map_err(|e| SecretError::Store(e.to_string()))?;
        match token {
            Some(t) => self.tokens().insert(account.clone(), t),
            None => self.tokens().remove(&account),
        };
        let store = self.secrets.clone();
        tokio::task::spawn_blocking(move || match json {
            Some(v) => store.set(&account, &v),
            None => store.delete(&account),
        })
        .await
        .map_err(|e| SecretError::Store(e.to_string()))?
    }

    fn tokens(&self) -> std::sync::MutexGuard<'_, HashMap<String, OAuth2Token>> {
        self.tokens.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
#[path = "secrets_tests.rs"]
mod tests;

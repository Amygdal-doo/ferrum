//! Registry credentials, kept where `docker login` keeps them: the
//! credential helper named in `config.json` (or the platform's), through the
//! helper protocol (`docker-credential-<name> get|store|erase|list`), or the
//! OS keychain when no helper is installed. Secrets are never written to a
//! file; inline `auths` left by an older `docker login` are read (for pulls)
//! and removed on sign-out, never created.
//!
//! Every call blocks (helpers may prompt), so the app runs them off the UI
//! thread; [`for_reference`] does that for pulls.

use crate::registry::{self, DockerConfig, RegistryAuth, SecretStore};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// A helper that has not answered by then is killed (a `pass` GPG prompt…).
const HELPER_TIMEOUT: Duration = Duration::from_secs(20);
/// Keychain service of the fallback store.
const KEYCHAIN_SERVICE: &str = "FerrumContainers registries";
/// Keychain account listing the fallback store's servers (`{server: username}`).
const KEYCHAIN_INDEX: &str = "_index";

/// Directories searched for `docker-credential-*` after `PATH` (relative to `$HOME` when not absolute).
const HELPER_DIRS: [&str; 7] = [
    "/usr/local/bin",
    "/opt/homebrew/bin",
    ".docker/bin",
    ".orbstack/bin",
    ".rd/bin",
    "/Applications/Docker.app/Contents/Resources/bin",
    "/Applications/OrbStack.app/Contents/MacOS/xbin",
];

/// The side effects behind [`Credentials`]: helper processes and the keychain.
pub trait SecretBackend: Send + Sync {
    fn helper_installed(&self, helper: &str) -> bool;
    /// Runs `docker-credential-<helper> <action>` with `input` on stdin and
    /// returns stdout; a failure carries the helper's message.
    fn helper(&self, helper: &str, action: &str, input: &str) -> Result<String, String>;
    fn keychain_get(&self, account: &str) -> Result<Option<String>, String>;
    fn keychain_set(&self, account: &str, secret: &str) -> Result<(), String>;
    /// Deleting a missing entry is not an error.
    fn keychain_delete(&self, account: &str) -> Result<(), String>;
}

/// A registry FerrumContainers holds credentials for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedIn {
    pub server: String,
    pub username: String,
    /// Where the secret is: `docker-credential-desktop`, `OS keychain`, `config.json`.
    pub store: String,
}

pub struct Credentials {
    /// The CLI's config folder (`$DOCKER_CONFIG` or `~/.docker`).
    config_dir: Option<PathBuf>,
    backend: Arc<dyn SecretBackend>,
}

/// Helper messages meaning "nothing stored" (osxkeychain, desktop, pass, secretservice).
fn is_not_found(message: &str) -> bool {
    let m = message.to_lowercase();
    m.contains("credentials not found") || m.contains("not found in") || m.contains("no such")
}

impl Credentials {
    #[must_use]
    pub fn new(config_dir: Option<PathBuf>, backend: Arc<dyn SecretBackend>) -> Self {
        Self {
            config_dir,
            backend,
        }
    }

    /// The real config folder, helpers and keychain.
    #[must_use]
    pub fn detect() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let docker_config = std::env::var("DOCKER_CONFIG").ok();
        Self::new(
            crate::context::config_dir(docker_config.as_deref(), home.as_deref()),
            Arc::new(SystemBackend::detect()),
        )
    }

    fn config_path(&self) -> Option<PathBuf> {
        self.config_dir.as_ref().map(|d| d.join("config.json"))
    }

    fn config(&self) -> DockerConfig {
        self.config_path()
            .and_then(|p| std::fs::read_to_string(p).ok())
            .map(|text| DockerConfig::parse(&text))
            .unwrap_or_default()
    }

    /// Where a sign-in to `server` would be kept.
    #[must_use]
    pub fn store_for(&self, server: &str) -> SecretStore {
        registry::store_for(&self.config(), server, |h| self.backend.helper_installed(h))
    }

    /// The credentials for `server` (a store key), from its store, else an
    /// inline `auths` entry of `config.json`.
    pub fn get(&self, server: &str) -> Result<Option<RegistryAuth>, String> {
        let config = self.config();
        let store = registry::store_for(&config, server, |h| self.backend.helper_installed(h));
        let stored = match &store {
            SecretStore::Helper(helper) => match self.backend.helper(helper, "get", server) {
                Ok(json) => parse_helper_get(server, &json),
                Err(message) if is_not_found(&message) => None,
                Err(message) => return Err(format!("{}: {message}", store.label())),
            },
            SecretStore::Keychain => self
                .backend
                .keychain_get(server)?
                .and_then(|json| parse_helper_get(server, &json)),
        };
        Ok(stored.or_else(|| config.inline_auth(server)))
    }

    /// Stores `auth` (already checked with the engine); returns where it went.
    pub fn save(&self, auth: &RegistryAuth) -> Result<SecretStore, String> {
        let store = self.store_for(&auth.server);
        let payload = serde_json::json!({
            "ServerURL": auth.server,
            "Username": auth.username,
            "Secret": auth.secret,
        })
        .to_string();
        match &store {
            SecretStore::Helper(helper) => {
                self.backend
                    .helper(helper, "store", &payload)
                    .map_err(|e| format!("{}: {e}", store.label()))?;
            }
            SecretStore::Keychain => {
                self.backend.keychain_set(&auth.server, &payload)?;
                let mut index = self.keychain_index();
                index.insert(auth.server.clone(), auth.username.clone());
                self.write_keychain_index(&index)?;
            }
        }
        Ok(store)
    }

    /// Signs out of `server`: erases it from its store and the keychain
    /// fallback, and drops its inline `auths` entry from `config.json`
    /// (what `docker logout` does).
    pub fn erase(&self, server: &str) -> Result<(), String> {
        let store = self.store_for(server);
        if let SecretStore::Helper(helper) = &store {
            match self.backend.helper(helper, "erase", server) {
                Ok(_) => {}
                Err(message) if is_not_found(&message) => {}
                Err(message) => return Err(format!("{}: {message}", store.label())),
            }
        }
        let mut index = self.keychain_index();
        if index.remove(server).is_some() {
            self.backend.keychain_delete(server)?;
            self.write_keychain_index(&index)?;
        }
        self.remove_inline(server)
    }

    /// Every registry with stored credentials, sorted, Docker Hub first.
    #[must_use]
    pub fn list(&self) -> Vec<SignedIn> {
        let config = self.config();
        let mut found: BTreeMap<String, SignedIn> = BTreeMap::new();
        let mut add = |server: String, username: String, store: String| {
            found.entry(server.clone()).or_insert(SignedIn {
                server,
                username,
                store,
            });
        };
        let mut helpers: Vec<String> = config.creds_store.iter().cloned().collect();
        if config.creds_store.is_none() {
            helpers.extend(
                registry::platform_helper()
                    .iter()
                    .filter(|h| self.backend.helper_installed(h))
                    .map(|h| (*h).to_owned()),
            );
        }
        for helper in helpers {
            match self.backend.helper(&helper, "list", "") {
                Ok(json) => {
                    for (server, username) in parse_helper_list(&json) {
                        let key = registry::server_key(&server);
                        // Per-registry helpers own their servers.
                        if !config.cred_helpers.contains_key(&key) {
                            add(key, username, SecretStore::Helper(helper.clone()).label());
                        }
                    }
                }
                Err(error) => tracing::debug!(%helper, %error, "credential helper list failed"),
            }
        }
        for (server, helper) in &config.cred_helpers {
            let store = SecretStore::Helper(helper.clone());
            if let Ok(json) = self.backend.helper(helper, "get", server)
                && let Some(auth) = parse_helper_get(server, &json)
            {
                add(server.clone(), auth.username, store.label());
            }
        }
        for (server, username) in self.keychain_index() {
            add(server, username, SecretStore::Keychain.label());
        }
        for server in config.auths.keys() {
            if let Some(auth) = config.inline_auth(server) {
                add(server.clone(), auth.username, "config.json".to_owned());
            }
        }
        let mut list: Vec<SignedIn> = found.into_values().collect();
        list.sort_by_key(|s| (s.server != registry::DOCKER_HUB, s.server.clone()));
        list
    }

    fn keychain_index(&self) -> BTreeMap<String, String> {
        self.backend
            .keychain_get(KEYCHAIN_INDEX)
            .ok()
            .flatten()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    fn write_keychain_index(&self, index: &BTreeMap<String, String>) -> Result<(), String> {
        if index.is_empty() {
            return self.backend.keychain_delete(KEYCHAIN_INDEX);
        }
        let json = serde_json::to_string(index).map_err(|e| e.to_string())?;
        self.backend.keychain_set(KEYCHAIN_INDEX, &json)
    }

    /// Removes `auths` entries of `server` (any spelling) from `config.json`,
    /// keeping everything else; nothing is written when there is none.
    fn remove_inline(&self, server: &str) -> Result<(), String> {
        let Some(path) = self.config_path() else {
            return Ok(());
        };
        let Ok(text) = std::fs::read_to_string(&path) else {
            return Ok(());
        };
        let Some(updated) = without_auth_entry(&text, server) else {
            return Ok(());
        };
        write_atomically(&path, &updated).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// `config.json` without the `auths` entries of `server`, or `None` when it has none.
#[must_use]
pub fn without_auth_entry(config_json: &str, server: &str) -> Option<String> {
    let mut value: serde_json::Value = serde_json::from_str(config_json).ok()?;
    let auths = value.get_mut("auths")?.as_object_mut()?;
    let keys: Vec<String> = auths
        .keys()
        .filter(|k| registry::server_key(k) == server)
        .cloned()
        .collect();
    if keys.is_empty() {
        return None;
    }
    for key in keys {
        auths.remove(&key);
    }
    let mut text = serde_json::to_string_pretty(&value).ok()?;
    text.push('\n');
    Some(text)
}

fn write_atomically(path: &Path, text: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.ferrum-tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, path)
}

/// A helper's `get` answer (`{"ServerURL","Username","Secret"}`).
fn parse_helper_get(server: &str, json: &str) -> Option<RegistryAuth> {
    let value: serde_json::Value = serde_json::from_str(json.trim()).ok()?;
    let secret = value.get("Secret")?.as_str()?.to_owned();
    if secret.is_empty() {
        return None;
    }
    Some(RegistryAuth {
        server: server.to_owned(),
        username: value
            .get("Username")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        secret,
    })
}

/// A helper's `list` answer (`{"server": "username"}`).
fn parse_helper_list(json: &str) -> Vec<(String, String)> {
    serde_json::from_str::<BTreeMap<String, String>>(json.trim())
        .map(|m| m.into_iter().collect())
        .unwrap_or_default()
}

/// The credentials a pull of `reference` should send, if any are stored.
/// Lookup failures are logged and mean an anonymous pull.
pub async fn for_reference(reference: &str) -> Option<RegistryAuth> {
    let server = registry::server_of(reference);
    let lookup = tokio::task::spawn_blocking(move || Credentials::detect().get(&server)).await;
    match lookup {
        Ok(Ok(auth)) => auth,
        Ok(Err(error)) => {
            tracing::warn!(%error, "registry credentials unavailable; pulling anonymously");
            None
        }
        Err(error) => {
            tracing::warn!(%error, "credential lookup failed");
            None
        }
    }
}

/// Helper processes found on `PATH` and in install folders, and the OS keychain.
pub struct SystemBackend {
    dirs: Vec<PathBuf>,
}

impl SystemBackend {
    #[must_use]
    pub fn detect() -> Self {
        let home = std::env::var_os("HOME").map(PathBuf::from);
        let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
            .map(|p| std::env::split_paths(&p).collect())
            .unwrap_or_default();
        dirs.extend(HELPER_DIRS.iter().filter_map(|d| {
            if d.starts_with('/') {
                Some(PathBuf::from(d))
            } else {
                home.as_ref().map(|h| h.join(d))
            }
        }));
        Self { dirs }
    }

    fn find(&self, helper: &str) -> Option<PathBuf> {
        // Helper names come from config.json: keep them to a plain file name.
        if helper.is_empty() || helper.contains(['/', '\\']) {
            return None;
        }
        let name = format!("docker-credential-{helper}");
        self.dirs
            .iter()
            .map(|d| d.join(&name))
            .find(|p| p.is_file())
    }
}

impl SecretBackend for SystemBackend {
    fn helper_installed(&self, helper: &str) -> bool {
        self.find(helper).is_some()
    }

    fn helper(&self, helper: &str, action: &str, input: &str) -> Result<String, String> {
        let path = self
            .find(helper)
            .ok_or_else(|| format!("docker-credential-{helper} is not installed"))?;
        let mut child = Command::new(&path)
            .arg(action)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| format!("could not run {}: {e}", path.display()))?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(input.as_bytes())
                .map_err(|e| e.to_string())?;
        }
        let read = |pipe: Option<Box<dyn Read + Send>>| {
            std::thread::spawn(move || {
                let mut text = String::new();
                if let Some(mut pipe) = pipe {
                    let _ = pipe.read_to_string(&mut text);
                }
                text
            })
        };
        let stdout = read(
            child
                .stdout
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let stderr = read(
            child
                .stderr
                .take()
                .map(|p| Box::new(p) as Box<dyn Read + Send>),
        );
        let deadline = Instant::now() + HELPER_TIMEOUT;
        let status = loop {
            match child.try_wait().map_err(|e| e.to_string())? {
                Some(status) => break status,
                None if Instant::now() > deadline => {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(format!("docker-credential-{helper} {action} timed out"));
                }
                None => std::thread::sleep(Duration::from_millis(20)),
            }
        };
        let out = stdout.join().unwrap_or_default();
        let err = stderr.join().unwrap_or_default();
        if status.success() {
            Ok(out)
        } else {
            let message = if err.trim().is_empty() { out } else { err };
            Err(message.trim().to_owned())
        }
    }

    fn keychain_get(&self, account: &str) -> Result<Option<String>, String> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| e.to_string())?;
        match entry.get_password() {
            Ok(secret) => Ok(Some(secret)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(e.to_string()),
        }
    }

    fn keychain_set(&self, account: &str, secret: &str) -> Result<(), String> {
        keyring::Entry::new(KEYCHAIN_SERVICE, account)
            .and_then(|entry| entry.set_password(secret))
            .map_err(|e| format!("OS keychain: {e}"))
    }

    fn keychain_delete(&self, account: &str) -> Result<(), String> {
        let entry = keyring::Entry::new(KEYCHAIN_SERVICE, account).map_err(|e| e.to_string())?;
        match entry.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(format!("OS keychain: {e}")),
        }
    }
}

/// In-memory [`SecretBackend`] for tests: helpers behave like the real
/// protocol over a map, and every helper call is recorded (without secrets).
#[derive(Default)]
pub struct MemoryBackend {
    pub installed: Vec<String>,
    pub helpers: std::sync::Mutex<BTreeMap<(String, String), (String, String)>>,
    pub keychain: std::sync::Mutex<BTreeMap<String, String>>,
    pub calls: std::sync::Mutex<Vec<String>>,
}

impl MemoryBackend {
    fn lock<T>(m: &std::sync::Mutex<T>) -> std::sync::MutexGuard<'_, T> {
        m.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    #[must_use]
    pub fn calls(&self) -> Vec<String> {
        Self::lock(&self.calls).clone()
    }
}

impl SecretBackend for MemoryBackend {
    fn helper_installed(&self, helper: &str) -> bool {
        self.installed.iter().any(|h| h == helper)
    }

    fn helper(&self, helper: &str, action: &str, input: &str) -> Result<String, String> {
        if !self.helper_installed(helper) {
            return Err(format!("docker-credential-{helper} is not installed"));
        }
        let mut store = Self::lock(&self.helpers);
        let call = match action {
            "store" => {
                let v: serde_json::Value =
                    serde_json::from_str(input).map_err(|e| e.to_string())?;
                let field = |k: &str| v.get(k).and_then(|s| s.as_str()).unwrap_or("").to_owned();
                let server = field("ServerURL");
                store.insert(
                    (helper.to_owned(), server.clone()),
                    (field("Username"), field("Secret")),
                );
                format!("{helper} store {server}")
            }
            _ => format!("{helper} {action} {input}"),
        };
        Self::lock(&self.calls).push(call.trim_end().to_owned());
        match action {
            "store" => Ok(String::new()),
            "get" => store
                .get(&(helper.to_owned(), input.to_owned()))
                .map(|(user, secret)| {
                    serde_json::json!({"ServerURL": input, "Username": user, "Secret": secret})
                        .to_string()
                })
                .ok_or_else(|| "credentials not found in native keychain".to_owned()),
            "erase" => store
                .remove(&(helper.to_owned(), input.to_owned()))
                .map(|_| String::new())
                .ok_or_else(|| "credentials not found in native keychain".to_owned()),
            "list" => {
                let map: BTreeMap<&String, &String> = store
                    .iter()
                    .filter(|((h, _), _)| h == helper)
                    .map(|((_, server), (user, _))| (server, user))
                    .collect();
                serde_json::to_string(&map).map_err(|e| e.to_string())
            }
            other => Err(format!("unknown action {other}")),
        }
    }

    fn keychain_get(&self, account: &str) -> Result<Option<String>, String> {
        Ok(Self::lock(&self.keychain).get(account).cloned())
    }

    fn keychain_set(&self, account: &str, secret: &str) -> Result<(), String> {
        Self::lock(&self.keychain).insert(account.to_owned(), secret.to_owned());
        Ok(())
    }

    fn keychain_delete(&self, account: &str) -> Result<(), String> {
        Self::lock(&self.keychain).remove(account);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::registry::DOCKER_HUB;

    fn auth(server: &str, user: &str, secret: &str) -> RegistryAuth {
        RegistryAuth {
            server: server.into(),
            username: user.into(),
            secret: secret.into(),
        }
    }

    fn setup(
        config: Option<&str>,
        installed: &[&str],
    ) -> (tempfile::TempDir, Arc<MemoryBackend>, Credentials) {
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("tempdir: {e}"));
        if let Some(text) = config {
            std::fs::write(dir.path().join("config.json"), text)
                .unwrap_or_else(|e| panic!("write: {e}"));
        }
        let backend = Arc::new(MemoryBackend {
            installed: installed.iter().map(|s| (*s).to_owned()).collect(),
            ..MemoryBackend::default()
        });
        let credentials = Credentials::new(Some(dir.path().to_path_buf()), backend.clone());
        (dir, backend, credentials)
    }

    #[test]
    fn signs_in_and_out_through_the_configured_helper() {
        let config = r#"{"auths":{},"credsStore":"desktop"}"#;
        let (dir, backend, credentials) = setup(Some(config), &["desktop"]);
        let store = credentials.save(&auth("ghcr.io", "me", "ghp_secret"));
        assert_eq!(store, Ok(SecretStore::Helper("desktop".into())));
        assert_eq!(
            credentials.get("ghcr.io"),
            Ok(Some(auth("ghcr.io", "me", "ghp_secret")))
        );
        assert_eq!(credentials.get("quay.io"), Ok(None));
        let list = credentials.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].store, "docker-credential-desktop");
        credentials
            .erase("ghcr.io")
            .unwrap_or_else(|e| panic!("erase: {e}"));
        assert_eq!(credentials.get("ghcr.io"), Ok(None));
        // Signing out of something not stored is fine.
        assert!(credentials.erase("ghcr.io").is_ok());
        // The secret never reached a file, and the call log has none either.
        let text = std::fs::read_to_string(dir.path().join("config.json")).unwrap_or_default();
        assert_eq!(text, config);
        assert!(backend.calls().iter().all(|c| !c.contains("ghp_secret")));
    }

    #[test]
    fn falls_back_to_the_keychain_without_a_helper() {
        let (dir, backend, credentials) = setup(None, &[]);
        let store = credentials.save(&auth(DOCKER_HUB, "me", "dckr_pat"));
        assert_eq!(store, Ok(SecretStore::Keychain));
        assert_eq!(
            credentials.get(DOCKER_HUB).ok().flatten().map(|a| a.secret),
            Some("dckr_pat".into())
        );
        assert_eq!(credentials.list()[0].store, "OS keychain");
        assert!(!dir.path().join("config.json").exists(), "nothing written");
        credentials
            .erase(DOCKER_HUB)
            .unwrap_or_else(|e| panic!("erase: {e}"));
        assert!(credentials.list().is_empty());
        assert!(MemoryBackend::lock(&backend.keychain).is_empty());
    }

    #[test]
    fn reads_and_removes_inline_auths_left_by_an_older_login() {
        let config = r#"{"auths":{"https://old.example.com":{"auth":"bWU6czNjcmV0"},"ghcr.io":{}},"currentContext":"orbstack"}"#;
        let (dir, _, credentials) = setup(Some(config), &[]);
        assert_eq!(
            credentials.get("old.example.com"),
            Ok(Some(auth("old.example.com", "me", "s3cret")))
        );
        assert_eq!(credentials.list()[0].store, "config.json");
        credentials
            .erase("old.example.com")
            .unwrap_or_else(|e| panic!("erase: {e}"));
        let text = std::fs::read_to_string(dir.path().join("config.json")).unwrap_or_default();
        assert!(!text.contains("old.example.com") && !text.contains("bWU6"));
        assert!(
            text.contains("ghcr.io") && text.contains("orbstack"),
            "{text}"
        );
        assert_eq!(without_auth_entry(&text, "old.example.com"), None);
    }

    #[test]
    fn per_registry_helpers_win() {
        let config = r#"{"credsStore":"desktop","credHelpers":{"ecr.example.com":"ecr-login"}}"#;
        let (_dir, backend, credentials) = setup(Some(config), &["desktop", "ecr-login"]);
        credentials
            .save(&auth("ecr.example.com", "AWS", "token"))
            .unwrap_or_else(|e| panic!("save: {e}"));
        assert!(
            backend
                .calls()
                .contains(&"ecr-login store ecr.example.com".to_owned())
        );
        let list = credentials.list();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].store, "docker-credential-ecr-login");
    }

    #[test]
    fn helper_errors_are_reported() {
        let config = r#"{"credsStore":"missing"}"#;
        let (_dir, _, credentials) = setup(Some(config), &[]);
        let error = credentials.get("ghcr.io").err().unwrap_or_default();
        assert!(error.contains("docker-credential-missing"), "{error}");
        assert!(credentials.save(&auth("ghcr.io", "me", "x")).is_err());
    }

    #[test]
    fn system_backend_refuses_paths_as_helper_names() {
        let backend = SystemBackend {
            dirs: vec![PathBuf::from("/usr/bin")],
        };
        assert!(!backend.helper_installed("../../bin/sh"));
        assert!(backend.helper("../sh", "get", "x").is_err());
    }
}

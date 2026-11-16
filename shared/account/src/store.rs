//! `~/.ferrum/account.json` (no secrets): the device id, the server, a cache
//! of the signed-in user and each app's [`AppMode`]. Written atomically
//! (temp file + rename) and re-read before every change, since all Ferrum
//! apps share it.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use uuid::Uuid;

use crate::api::Me;
use crate::{AccountError, App, AppMode, Result};

/// The account server unless [`SERVER_ENV`] says otherwise.
pub const DEFAULT_SERVER: &str = "https://tryferrum.dev";
/// Overrides the server, e.g. `http://localhost:8080`.
pub const SERVER_ENV: &str = "FERRUM_ACCOUNT_SERVER";
pub const FILE_NAME: &str = "account.json";

/// `~/.ferrum`, or `$FERRUM_HOME` when set (as in `ferrum-ui`'s prefs).
#[must_use]
pub fn ferrum_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FERRUM_HOME") {
        return Some(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"))?;
    Some(Path::new(&home).join(".ferrum"))
}

/// The server base URL (no trailing `/`): [`SERVER_ENV`] or [`DEFAULT_SERVER`].
pub fn server_url(env: impl Fn(&str) -> Option<String>) -> String {
    env(SERVER_ENV)
        .map(|s| s.trim().trim_end_matches('/').to_owned())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| DEFAULT_SERVER.to_owned())
}

/// The contents of `account.json`. Unknown keys are kept.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AccountFile {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub device_id: Option<Uuid>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<String>,
    /// The launcher's user, as last fetched from `/api/v1/me`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<Me>,
    pub apps: BTreeMap<String, AppMode>,
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl AccountFile {
    /// The app's mode; [`AppMode::FollowLauncher`] when unset.
    #[must_use]
    pub fn mode(&self, app: App) -> AppMode {
        self.apps.get(app.id()).copied().unwrap_or_default()
    }

    pub fn set_mode(&mut self, app: App, mode: AppMode) {
        self.apps.insert(app.id().to_owned(), mode);
    }

    /// The device id, made now if there is none yet.
    pub fn device_id(&mut self) -> Uuid {
        *self.device_id.get_or_insert_with(Uuid::new_v4)
    }
}

/// `account.json` at a fixed path.
#[derive(Debug, Clone)]
pub struct FileStore {
    path: PathBuf,
}

impl FileStore {
    #[must_use]
    pub fn new(dir: &Path) -> Self {
        Self {
            path: dir.join(FILE_NAME),
        }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    fn error(&self, e: impl std::fmt::Display) -> AccountError {
        AccountError::File {
            path: self.path.display().to_string(),
            message: e.to_string(),
        }
    }

    /// The file's contents; the default when it is missing. A file that is
    /// not valid JSON is treated as empty (and replaced on the next write).
    pub fn read(&self) -> Result<AccountFile> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => Ok(serde_json::from_str(&text).unwrap_or_else(|e| {
                tracing::warn!(path = %self.path.display(), error = %e, "ignoring an unreadable account.json");
                AccountFile::default()
            })),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(AccountFile::default()),
            Err(e) => Err(self.error(e)),
        }
    }

    /// Replaces the file atomically.
    pub fn write(&self, file: &AccountFile) -> Result<()> {
        let dir = self
            .path
            .parent()
            .ok_or_else(|| self.error("no parent directory"))?;
        std::fs::create_dir_all(dir).map_err(|e| self.error(e))?;
        let json = serde_json::to_string_pretty(file).map_err(|e| self.error(e))?;
        let tmp = dir.join(format!(
            ".{FILE_NAME}.{}.{}.tmp",
            std::process::id(),
            Uuid::new_v4().simple()
        ));
        std::fs::write(&tmp, json + "\n").map_err(|e| self.error(e))?;
        std::fs::rename(&tmp, &self.path).map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            self.error(e)
        })
    }

    /// Reads, applies `change`, writes back, and returns what `change`
    /// returned.
    pub fn update<T>(&self, change: impl FnOnce(&mut AccountFile) -> T) -> Result<T> {
        let mut file = self.read()?;
        let out = change(&mut file);
        self.write(&file)?;
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::Organization;

    fn me() -> Me {
        Me {
            id: "u1".into(),
            username: "ada".into(),
            name: Some("Ada".into()),
            email: Some("ada@example.com".into()),
            avatar_url: Some("https://avatars.example/1".into()),
            account_url: "https://tryferrum.dev/account".into(),
            organizations: vec![Organization {
                id: "o1".into(),
                name: "ada".into(),
                plan: "free".into(),
                personal: true,
                role: "owner".into(),
            }],
        }
    }

    #[test]
    fn round_trips_and_keeps_unknown_keys() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileStore::new(dir.path());
        assert_eq!(
            store.read().expect("missing is empty"),
            AccountFile::default()
        );

        std::fs::write(
            store.path(),
            r#"{"future_key": [1, 2], "apps": {"editor": "own_account"}}"#,
        )
        .expect("seed");
        let id = store
            .update(|file| {
                file.server = Some(DEFAULT_SERVER.into());
                file.user = Some(me());
                file.set_mode(App::Git, AppMode::SignedOut);
                file.device_id()
            })
            .expect("update");

        let file = store.read().expect("read");
        assert_eq!(file.device_id, Some(id));
        assert_eq!(file.user, Some(me()));
        assert_eq!(file.mode(App::Editor), AppMode::OwnAccount);
        assert_eq!(file.mode(App::Git), AppMode::SignedOut);
        assert_eq!(file.mode(App::Term), AppMode::FollowLauncher);
        assert_eq!(file.extra["future_key"], serde_json::json!([1, 2]));
        // The device id is stable.
        assert_eq!(store.update(AccountFile::device_id).expect("again"), id);
        // No temp files are left behind.
        let names: Vec<_> = std::fs::read_dir(dir.path())
            .expect("ls")
            .map(|e| e.expect("entry").file_name())
            .collect();
        assert_eq!(names, vec![std::ffi::OsString::from(FILE_NAME)]);
    }

    #[test]
    fn garbage_is_treated_as_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let store = FileStore::new(dir.path());
        std::fs::write(store.path(), "not json").expect("seed");
        assert_eq!(store.read().expect("read"), AccountFile::default());
    }

    #[test]
    fn server_from_env_or_default() {
        assert_eq!(server_url(|_| None), DEFAULT_SERVER);
        assert_eq!(
            server_url(|_| Some(" http://localhost:8080/ ".into())),
            "http://localhost:8080"
        );
        assert_eq!(server_url(|_| Some("  ".into())), DEFAULT_SERVER);
    }
}

//! SSH hosts: the ones saved in FerrumTerm (`ssh-hosts.json` in the state
//! directory) and the aliases from `~/.ssh/config`, plus saved port-forward
//! definitions per host. FerrumTerm never stores passwords or keys: only an
//! identity *file path*, and ssh / ssh-agent handle passphrases.

use crate::ssh_config::ConfigHost;
use ferrumterm_protocol::ForwardSpec;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// A host saved in FerrumTerm.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SavedHost {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user: Option<String>,
    pub host: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_file: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jump_host: Option<String>,
    /// Run on the remote host instead of a login shell (`ssh -t host <command>`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub startup_command: Option<String>,
}

/// Raw text of the host form (GUI) before validation.
#[derive(Debug, Clone, Copy, Default)]
pub struct HostFormFields<'a> {
    pub name: &'a str,
    pub user: &'a str,
    pub host: &'a str,
    pub port: &'a str,
    pub identity_file: &'a str,
    pub jump_host: &'a str,
    pub startup_command: &'a str,
}

fn bad_token(s: &str) -> bool {
    s.is_empty() || s.starts_with('-') || s.chars().any(|c| c.is_whitespace() || c.is_control())
}

impl SavedHost {
    /// Checks fields that end up as ssh arguments (no option injection).
    pub fn validate(&self) -> Result<(), String> {
        if self.name.trim().is_empty() {
            return Err("name is required".into());
        }
        if self.name.contains(['/', '\n']) {
            return Err("name can't contain “/”".into());
        }
        if bad_token(&self.host) {
            return Err("host is required (no spaces, can't start with “-”)".into());
        }
        if self.user.as_deref().is_some_and(bad_token) {
            return Err("user can't contain spaces or start with “-”".into());
        }
        if self.jump_host.as_deref().is_some_and(bad_token) {
            return Err("jump host can't contain spaces or start with “-”".into());
        }
        if self.port == Some(0) {
            return Err("port must be 1–65535".into());
        }
        if self
            .identity_file
            .as_deref()
            .is_some_and(|f| f.is_empty() || f.starts_with('-'))
        {
            return Err("identity file can't start with “-”".into());
        }
        Ok(())
    }

    /// Builds a host from form text fields (trimmed; empty = unset).
    /// `host` may be `user@host`, which fills `user` when that is empty.
    pub fn from_form(f: &HostFormFields<'_>) -> Result<Self, String> {
        let opt = |s: &str| Some(s.trim().to_owned()).filter(|s| !s.is_empty());
        let (mut user, host) = match f.host.trim().rsplit_once('@') {
            Some((u, h)) => (opt(u), h.to_owned()),
            None => (None, f.host.trim().to_owned()),
        };
        if let Some(u) = opt(f.user) {
            user = Some(u);
        }
        let port = match opt(f.port) {
            None => None,
            Some(p) => Some(
                p.parse::<u16>()
                    .ok()
                    .filter(|p| *p > 0)
                    .ok_or_else(|| format!("port “{p}” must be 1–65535"))?,
            ),
        };
        let h = Self {
            name: f.name.trim().to_owned(),
            user,
            host,
            port,
            identity_file: opt(f.identity_file),
            jump_host: opt(f.jump_host),
            startup_command: opt(f.startup_command),
        };
        h.validate()?;
        Ok(h)
    }

    /// `user@host:port` for display.
    #[must_use]
    pub fn address(&self) -> String {
        let mut s = match &self.user {
            Some(u) => format!("{u}@{}", self.host),
            None => self.host.clone(),
        };
        if let Some(p) = self.port.filter(|p| *p != 22) {
            s.push_str(&format!(":{p}"));
        }
        s
    }
}

/// Where a listed host comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSource {
    Saved(SavedHost),
    /// From `~/.ssh/config` (read-only).
    SshConfig(ConfigHost),
}

/// A host as shown in the sidebar / `fterm ssh --list`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub name: String,
    pub source: HostSource,
}

impl Host {
    #[must_use]
    pub fn is_saved(&self) -> bool {
        matches!(self.source, HostSource::Saved(_))
    }

    /// `deploy@10.0.0.5:2222`, or the alias when the config says nothing.
    #[must_use]
    pub fn address(&self) -> String {
        match &self.source {
            HostSource::Saved(h) => h.address(),
            HostSource::SshConfig(c) => {
                let host = c.hostname.clone().unwrap_or_else(|| c.alias.clone());
                let mut s = match &c.user {
                    Some(u) => format!("{u}@{host}"),
                    None => host,
                };
                if let Some(p) = c.port.filter(|p| *p != 22) {
                    s.push_str(&format!(":{p}"));
                }
                s
            }
        }
    }

    #[must_use]
    pub fn source_name(&self) -> &'static str {
        match self.source {
            HostSource::Saved(_) => "saved",
            HostSource::SshConfig(_) => "ssh_config",
        }
    }
}

/// `ssh-hosts.json`: saved hosts and forward definitions (by host name,
/// for saved and ssh-config hosts alike).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HostStore {
    pub hosts: Vec<SavedHost>,
    pub forwards: BTreeMap<String, Vec<ForwardSpec>>,
}

/// Default location: `<state dir>/ssh-hosts.json`.
#[must_use]
pub fn store_path(state_dir: &Path) -> PathBuf {
    state_dir.join("ssh-hosts.json")
}

impl HostStore {
    /// Loads the store; a missing file is an empty store, a corrupt one an error.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} is not valid: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("could not read {}: {e}", path.display())),
        }
    }

    /// Writes atomically (temp file + rename), mode 0600.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        use std::os::unix::fs::PermissionsExt;
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(|e| e.to_string())?;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<&SavedHost> {
        self.hosts.iter().find(|h| h.name == name)
    }

    /// Adds or replaces a host. `previous` is its old name when renaming
    /// (its forwards move along).
    pub fn upsert(&mut self, host: SavedHost, previous: Option<&str>) -> Result<(), String> {
        host.validate()?;
        let old = previous.unwrap_or(&host.name).to_owned();
        if old != host.name && self.get(&host.name).is_some() {
            return Err(format!("a host named “{}” already exists", host.name));
        }
        if old != host.name
            && let Some(f) = self.forwards.remove(&old)
        {
            self.forwards.insert(host.name.clone(), f);
        }
        match self.hosts.iter_mut().find(|h| h.name == old) {
            Some(slot) => *slot = host,
            None => self.hosts.push(host),
        }
        self.hosts.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(())
    }

    /// Removes a saved host and its forwards.
    pub fn remove(&mut self, name: &str) -> bool {
        let before = self.hosts.len();
        self.hosts.retain(|h| h.name != name);
        self.forwards.remove(name);
        self.hosts.len() != before
    }

    #[must_use]
    pub fn forwards_of(&self, host: &str) -> &[ForwardSpec] {
        self.forwards.get(host).map_or(&[], Vec::as_slice)
    }

    /// Replaces a host's forwards (ids must be unique).
    pub fn set_forwards(&mut self, host: &str, specs: Vec<ForwardSpec>) -> Result<(), String> {
        let mut ids: Vec<String> = specs.iter().map(ForwardSpec::id).collect();
        ids.sort();
        if let Some(w) = ids.windows(2).find(|w| w[0] == w[1]) {
            return Err(format!("two forwards use {}", w[0]));
        }
        if specs.is_empty() {
            self.forwards.remove(host);
        } else {
            self.forwards.insert(host.to_owned(), specs);
        }
        Ok(())
    }

    /// Adds one forward, replacing any with the same id.
    pub fn add_forward(&mut self, host: &str, spec: ForwardSpec) {
        let list = self.forwards.entry(host.to_owned()).or_default();
        list.retain(|f| f.id() != spec.id());
        list.push(spec);
    }

    /// Removes a forward by id (`L8080`) or spec text.
    pub fn remove_forward(&mut self, host: &str, id: &str) -> bool {
        let Some(list) = self.forwards.get_mut(host) else {
            return false;
        };
        let before = list.len();
        let parsed = ForwardSpec::parse(id).ok();
        list.retain(|f| !(f.id().eq_ignore_ascii_case(id) || Some(f) == parsed.as_ref()));
        let removed = list.len() != before;
        if list.is_empty() {
            self.forwards.remove(host);
        }
        removed
    }

    /// Saved hosts first (sorted), then ssh-config aliases not shadowed by a saved name.
    #[must_use]
    pub fn all_hosts(&self, config: &[ConfigHost]) -> Vec<Host> {
        let mut out: Vec<Host> = self
            .hosts
            .iter()
            .map(|h| Host {
                name: h.name.clone(),
                source: HostSource::Saved(h.clone()),
            })
            .collect();
        for c in config {
            if !out.iter().any(|h| h.name == c.alias) {
                out.push(Host {
                    name: c.alias.clone(),
                    source: HostSource::SshConfig(c.clone()),
                });
            }
        }
        out
    }
}

/// Finds a host by name among saved and ssh-config hosts.
#[must_use]
pub fn find_host(store: &HostStore, config: &[ConfigHost], name: &str) -> Option<Host> {
    store.all_hosts(config).into_iter().find(|h| h.name == name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn host(name: &str) -> SavedHost {
        SavedHost {
            name: name.into(),
            host: format!("{name}.example.com"),
            ..SavedHost::default()
        }
    }

    #[test]
    fn validates_fields_that_become_arguments() {
        assert!(host("a").validate().is_ok());
        let mut h = host("a");
        h.host = "-oProxyCommand=evil".into();
        assert!(h.validate().is_err());
        let mut h = host("a");
        h.user = Some("me you".into());
        assert!(h.validate().is_err());
        let mut h = host("a");
        h.port = Some(0);
        assert!(h.validate().is_err());
        let mut h = host("a/b");
        h.host = "x".into();
        assert!(h.validate().is_err());
        assert!(host(" ").validate().is_err());
    }

    #[test]
    fn builds_hosts_from_form_text() {
        let h = SavedHost::from_form(&HostFormFields {
            name: " box ",
            host: "me@box.example.com",
            port: " 2222",
            identity_file: "~/.ssh/id_ed25519",
            ..HostFormFields::default()
        })
        .expect("host");
        assert_eq!(h.name, "box");
        assert_eq!(h.user.as_deref(), Some("me"));
        assert_eq!(h.host, "box.example.com");
        assert_eq!(h.port, Some(2222));
        assert_eq!(h.jump_host, None);
        let explicit = SavedHost::from_form(&HostFormFields {
            name: "b",
            user: "root",
            host: "me@b",
            ..HostFormFields::default()
        })
        .expect("host");
        assert_eq!(explicit.user.as_deref(), Some("root"), "user field wins");
        for bad in ["0", "99999", "x"] {
            assert!(
                SavedHost::from_form(&HostFormFields {
                    name: "b",
                    host: "b",
                    port: bad,
                    ..HostFormFields::default()
                })
                .is_err()
            );
        }
        assert!(SavedHost::from_form(&HostFormFields::default()).is_err());
    }

    #[test]
    fn upsert_rename_remove_and_forwards() {
        let mut s = HostStore::default();
        s.upsert(host("web"), None).expect("add");
        s.upsert(host("db"), None).expect("add");
        assert_eq!(
            s.hosts.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(),
            ["db", "web"]
        );
        let l = ForwardSpec::parse("L 5432:localhost:5432").expect("spec");
        s.add_forward("db", l.clone());
        s.add_forward("db", ForwardSpec::parse("D 1080").expect("spec"));
        assert_eq!(s.forwards_of("db").len(), 2);
        // Renaming carries forwards along; clashing names are refused.
        let mut renamed = host("postgres");
        renamed.port = Some(2200);
        s.upsert(renamed, Some("db")).expect("rename");
        assert!(s.get("db").is_none());
        assert_eq!(s.forwards_of("postgres").len(), 2);
        assert!(s.upsert(host("web"), Some("postgres")).is_err());
        assert!(s.remove_forward("postgres", "l5432"));
        assert!(s.remove_forward("postgres", "D 1080"));
        assert!(!s.forwards.contains_key("postgres"));
        assert!(
            s.set_forwards(
                "web",
                ForwardSpec::parse_list("L 80:a:80, L 80:b:80").expect("list")
            )
            .is_err()
        );
        assert!(s.remove("web"));
        assert!(!s.remove("web"));
    }

    #[test]
    fn saves_and_loads_with_private_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().expect("tmp");
        let path = store_path(dir.path());
        assert_eq!(
            HostStore::load(&path).expect("missing"),
            HostStore::default()
        );
        let mut s = HostStore::default();
        let mut h = host("box");
        h.user = Some("me".into());
        h.identity_file = Some("~/.ssh/id_ed25519".into());
        h.startup_command = Some("tmux new -A -s main".into());
        s.upsert(h, None).expect("add");
        s.add_forward(
            "box",
            ForwardSpec::parse("R 9000:localhost:3000").expect("spec"),
        );
        s.save(&path).expect("save");
        assert_eq!(HostStore::load(&path).expect("load"), s);
        let mode = std::fs::metadata(&path).expect("meta").permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        std::fs::write(&path, "{not json").expect("write");
        assert!(HostStore::load(&path).is_err());
    }

    #[test]
    fn merges_saved_and_config_hosts() {
        let mut s = HostStore::default();
        s.upsert(host("prod"), None).expect("add");
        let config = vec![
            ConfigHost {
                alias: "prod".into(),
                ..ConfigHost::default()
            },
            ConfigHost {
                alias: "pi".into(),
                hostname: Some("10.0.0.9".into()),
                user: Some("pi".into()),
                port: Some(2222),
                ..ConfigHost::default()
            },
        ];
        let all = s.all_hosts(&config);
        assert_eq!(all.len(), 2);
        assert!(all[0].is_saved(), "saved hosts shadow config aliases");
        assert_eq!(all[1].address(), "pi@10.0.0.9:2222");
        assert_eq!(all[1].source_name(), "ssh_config");
        assert!(find_host(&s, &config, "pi").is_some());
        assert!(find_host(&s, &config, "nope").is_none());
    }
}

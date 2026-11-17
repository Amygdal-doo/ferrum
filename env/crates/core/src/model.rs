//! The decrypted vault: projects → environments → variables, plus
//! settings. Variable keys, values and comments are wiped on drop, and
//! `Debug` never prints values.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use zeroize::{Zeroize, ZeroizeOnDrop};

use crate::identity::{Identity, Member, PublicKey};
use crate::share_key::ShareKey;
use crate::sync::SyncState;
use crate::{Error, Result};

/// Current vault format version (inside the ciphertext). 2 added the
/// identity and project members: older builds refuse such a vault instead
/// of dropping the private key on their next save.
pub const VAULT_VERSION: u32 = 2;
/// Environments a new project starts with.
pub const DEFAULT_ENVIRONMENTS: [&str; 3] = ["development", "staging", "production"];
/// Auto-lock choices offered in settings, in minutes.
pub const AUTO_LOCK_CHOICES: [u32; 4] = [5, 15, 30, 60];

#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, Zeroize, ZeroizeOnDrop)]
pub struct Variable {
    pub key: String,
    pub value: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub comment: String,
    /// Masked in the UI until revealed.
    #[serde(default = "yes")]
    pub secret: bool,
}

fn yes() -> bool {
    true
}

impl Variable {
    #[must_use]
    pub fn new(key: impl Into<String>, value: impl Into<String>) -> Self {
        Self {
            key: key.into(),
            value: value.into(),
            comment: String::new(),
            secret: true,
        }
    }
}

impl std::fmt::Debug for Variable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Variable")
            .field("key", &self.key)
            .field("value", &"<redacted>")
            .field("secret", &self.secret)
            .finish_non_exhaustive()
    }
}

/// `true` for keys usable in a `.env` file and a shell: `[A-Za-z_][A-Za-z0-9_.-]*`.
#[must_use]
pub fn is_valid_key(key: &str) -> bool {
    let mut chars = key.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '.' | '-'))
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Environment {
    pub name: String,
    #[serde(default)]
    pub variables: Vec<Variable>,
}

impl Environment {
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            variables: Vec::new(),
        }
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&Variable> {
        self.variables.iter().find(|v| v.key == key)
    }

    /// Adds `var`, or replaces the variable at `index` (an edit, which may
    /// rename the key). Fails on an invalid or duplicate key.
    pub fn set(&mut self, index: Option<usize>, mut var: Variable) -> Result<usize> {
        var.key = var.key.trim().to_string();
        if !is_valid_key(&var.key) {
            return Err(Error::Invalid(format!(
                "\"{}\" is not a valid name: use letters, digits and _ (not starting with a digit)",
                var.key
            )));
        }
        let clash = self
            .variables
            .iter()
            .position(|v| v.key == var.key)
            .filter(|&i| Some(i) != index);
        if clash.is_some() {
            return Err(Error::Invalid(format!("{} already exists", var.key)));
        }
        match index.filter(|&i| i < self.variables.len()) {
            Some(i) => {
                self.variables[i] = var;
                Ok(i)
            }
            None => {
                self.variables.push(var);
                Ok(self.variables.len() - 1)
            }
        }
    }

    /// Sets `key` to `var`, replacing an existing one in place (imports).
    pub fn upsert(&mut self, var: Variable) {
        match self.variables.iter_mut().find(|v| v.key == var.key) {
            Some(slot) => *slot = var,
            None => self.variables.push(var),
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<Variable> {
        (index < self.variables.len()).then(|| self.variables.remove(index))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    /// The folder this project belongs to; `None` for a free-named project.
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub share_key: Option<ShareKey>,
    /// Teammates whose public keys bundles are encrypted to (you are
    /// always added as a recipient too).
    #[serde(default)]
    pub members: Vec<Member>,
    /// The shared folder team bundles are synced through.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sync: Option<SyncState>,
    #[serde(default)]
    pub environments: Vec<Environment>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Project {
    #[must_use]
    pub fn new(name: impl Into<String>, path: Option<PathBuf>) -> Self {
        let now = Utc::now();
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            name: name.into(),
            path,
            share_key: None,
            members: Vec::new(),
            sync: None,
            environments: DEFAULT_ENVIRONMENTS
                .iter()
                .map(|n| Environment::new(*n))
                .collect(),
            created_at: now,
            updated_at: now,
        }
    }

    #[must_use]
    pub fn environment_index(&self, name: &str) -> Option<usize> {
        self.environments
            .iter()
            .position(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// The environment called `name`, created when missing.
    pub fn environment_mut(&mut self, name: &str) -> &mut Environment {
        let index = match self.environment_index(name) {
            Some(i) => i,
            None => {
                self.environments.push(Environment::new(name));
                self.environments.len() - 1
            }
        };
        &mut self.environments[index]
    }

    pub fn add_environment(&mut self, name: &str) -> Result<usize> {
        let name = clean_name(name, "environment")?;
        if self.environment_index(&name).is_some() {
            return Err(Error::Invalid(format!("{name} already exists")));
        }
        self.environments.push(Environment::new(name));
        Ok(self.environments.len() - 1)
    }

    pub fn rename_environment(&mut self, index: usize, name: &str) -> Result<()> {
        let name = clean_name(name, "environment")?;
        if self.environment_index(&name).is_some_and(|i| i != index) {
            return Err(Error::Invalid(format!("{name} already exists")));
        }
        let env = self
            .environments
            .get_mut(index)
            .ok_or_else(|| Error::Invalid("no such environment".into()))?;
        env.name = name;
        Ok(())
    }

    pub fn touch(&mut self) {
        self.updated_at = Utc::now();
    }

    /// Adds a teammate's key. Names and keys are unique; your own key is
    /// refused (you are always a recipient).
    pub fn add_member(&mut self, name: &str, key: PublicKey, me: Option<&PublicKey>) -> Result<()> {
        let name = clean_name(name, "member")?;
        if me == Some(&key) {
            return Err(Error::Invalid(
                "That is your own key: you always receive your bundles.".into(),
            ));
        }
        if let Some(other) = self.member(&key) {
            return Err(Error::Invalid(format!(
                "That key is already in the team as {}",
                other.name
            )));
        }
        if self
            .members
            .iter()
            .any(|m| m.name.eq_ignore_ascii_case(&name))
        {
            return Err(Error::Invalid(format!("{name} is already in the team")));
        }
        self.members.push(Member {
            name,
            public_key: key,
        });
        Ok(())
    }

    #[must_use]
    pub fn member(&self, key: &PublicKey) -> Option<&Member> {
        self.members.iter().find(|m| m.public_key == *key)
    }

    /// Everyone a team bundle goes to: you first, then the members.
    #[must_use]
    pub fn recipients(&self, me: &PublicKey) -> Vec<PublicKey> {
        std::iter::once(*me)
            .chain(self.members.iter().map(|m| m.public_key))
            .collect()
    }
}

fn clean_name(name: &str, what: &str) -> Result<String> {
    let name = name.trim();
    if name.is_empty() {
        return Err(Error::Invalid(format!("The {what} needs a name")));
    }
    if name.chars().any(char::is_control) || name.len() > 100 {
        return Err(Error::Invalid(format!("That {what} name is not allowed")));
    }
    Ok(name.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default = "default_auto_lock")]
    pub auto_lock_minutes: u32,
}

fn default_auto_lock() -> u32 {
    30
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            auto_lock_minutes: default_auto_lock(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vault {
    pub version: u32,
    #[serde(default)]
    pub settings: Settings,
    /// Your key pair for team bundles; created on first use.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<Identity>,
    #[serde(default)]
    pub projects: Vec<Project>,
}

impl Default for Vault {
    fn default() -> Self {
        Self {
            version: VAULT_VERSION,
            settings: Settings::default(),
            identity: None,
            projects: Vec::new(),
        }
    }
}

impl Vault {
    /// Your identity, generated when the vault has none yet (the caller
    /// saves the vault).
    pub fn ensure_identity(&mut self) -> Result<&Identity> {
        let identity = match self.identity.take() {
            Some(identity) => identity,
            None => Identity::generate()?,
        };
        Ok(self.identity.insert(identity))
    }

    /// Adds a project (names are unique, case-insensitively) and returns
    /// its index.
    pub fn add_project(&mut self, name: &str, path: Option<PathBuf>) -> Result<usize> {
        let name = clean_name(name, "project")?;
        if self.project_by_name(&name).is_some() {
            return Err(Error::Invalid(format!(
                "A project named {name} already exists"
            )));
        }
        if let Some(path) = &path
            && self.project_by_path(path).is_some()
        {
            return Err(Error::Invalid(format!(
                "{} already has a project",
                path.display()
            )));
        }
        self.projects.push(Project::new(name, path));
        Ok(self.projects.len() - 1)
    }

    #[must_use]
    pub fn project_by_name(&self, name: &str) -> Option<usize> {
        self.projects
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(name.trim()))
    }

    #[must_use]
    pub fn project_by_path(&self, path: &Path) -> Option<usize> {
        self.projects
            .iter()
            .position(|p| p.path.as_deref() == Some(path))
    }

    /// The project for `path`, created (named after the folder, made
    /// unique) when there is none.
    pub fn project_for_folder(&mut self, path: &Path) -> usize {
        if let Some(index) = self.project_by_path(path) {
            return index;
        }
        let base = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        );
        let mut name = base.clone();
        let mut n = 2;
        while self.project_by_name(&name).is_some() {
            name = format!("{base} ({n})");
            n += 1;
        }
        self.projects
            .push(Project::new(name, Some(path.to_path_buf())));
        self.projects.len() - 1
    }

    /// Indices of projects whose name or path contains `query`.
    #[must_use]
    pub fn search(&self, query: &str) -> Vec<usize> {
        let query = query.trim().to_lowercase();
        self.projects
            .iter()
            .enumerate()
            .filter(|(_, p)| {
                query.is_empty()
                    || p.name.to_lowercase().contains(&query)
                    || p.path
                        .as_ref()
                        .is_some_and(|x| x.to_string_lossy().to_lowercase().contains(&query))
            })
            .map(|(i, _)| i)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_are_validated() {
        for ok in ["A", "_x", "DATABASE_URL", "a.b-c", "K9"] {
            assert!(is_valid_key(ok), "{ok}");
        }
        for bad in ["", "9A", "A B", "A=B", "ÄÖ", "-x"] {
            assert!(!is_valid_key(bad), "{bad}");
        }
    }

    #[test]
    fn environment_set_add_edit_rename_and_clash() {
        let mut env = Environment::new("dev");
        assert_eq!(env.set(None, Variable::new(" A ", "1")).expect("add"), 0);
        assert_eq!(env.set(None, Variable::new("B", "2")).expect("add"), 1);
        assert!(env.set(None, Variable::new("A", "x")).is_err());
        assert!(env.set(None, Variable::new("1A", "x")).is_err());
        // Editing A in place keeps its slot; renaming onto B is refused.
        env.set(Some(0), Variable::new("A", "one")).expect("edit");
        assert_eq!(env.get("A").expect("A").value, "one");
        assert!(env.set(Some(0), Variable::new("B", "x")).is_err());
        env.set(Some(0), Variable::new("C", "one")).expect("rename");
        assert_eq!(env.variables[0].key, "C");
        env.upsert(Variable::new("B", "22"));
        env.upsert(Variable::new("D", "4"));
        assert_eq!(env.variables.len(), 3);
        assert_eq!(env.get("B").expect("B").value, "22");
        assert!(env.remove(9).is_none());
        assert_eq!(env.remove(0).expect("removed").key, "C");
    }

    #[test]
    fn projects_and_environments() {
        let mut vault = Vault::default();
        let i = vault.add_project("Shop", None).expect("add");
        assert!(vault.add_project("shop", None).is_err());
        assert!(vault.add_project("  ", None).is_err());
        let p = &mut vault.projects[i];
        assert_eq!(p.environments.len(), 3);
        assert!(p.add_environment("Staging").is_err());
        let qa = p.add_environment("qa").expect("qa");
        assert!(p.rename_environment(qa, "production").is_err());
        p.rename_environment(qa, "QA").expect("rename");
        p.environment_mut("preview").upsert(Variable::new("X", "1"));
        assert_eq!(p.environments.len(), 5);

        let folder = PathBuf::from("/code/shop");
        let a = vault.project_for_folder(&folder);
        assert_eq!(vault.projects[a].name, "shop (2)");
        assert_eq!(vault.project_for_folder(&folder), a);
        assert!(vault.add_project("Other", Some(folder)).is_err());
        assert_eq!(vault.search("SHO"), vec![0, 1]);
        assert_eq!(vault.search("/code"), vec![1]);
    }

    #[test]
    fn debug_hides_values() {
        let var = Variable::new("TOKEN", "s3cr3t-value");
        let env = Environment {
            name: "dev".into(),
            variables: vec![var],
        };
        let text = format!("{env:?}");
        assert!(text.contains("TOKEN"));
        assert!(!text.contains("s3cr3t-value"));
    }

    #[test]
    fn team_members() {
        let mut vault = Vault::default();
        let me = vault
            .ensure_identity()
            .expect("identity")
            .public_key()
            .expect("pk");
        let again = vault
            .ensure_identity()
            .expect("identity")
            .public_key()
            .expect("pk");
        assert_eq!(me, again, "generated once");
        let ana = Identity::generate().expect("id").public_key().expect("pk");
        let bo = Identity::generate().expect("id").public_key().expect("pk");
        let i = vault.add_project("shop", None).expect("project");
        let p = &mut vault.projects[i];
        assert!(p.add_member("Me", me, Some(&me)).is_err());
        p.add_member(" Ana ", ana, Some(&me)).expect("ana");
        assert!(p.add_member("Ana 2", ana, Some(&me)).is_err());
        assert!(p.add_member("ana", bo, Some(&me)).is_err());
        p.add_member("Bo", bo, Some(&me)).expect("bo");
        assert_eq!(p.member(&ana).expect("member").name, "Ana");
        assert_eq!(p.recipients(&me), vec![me, ana, bo]);

        // Identity and members survive a round trip; old vaults load.
        let json = serde_json::to_string(&vault).expect("ser");
        let back: Vault = serde_json::from_str(&json).expect("de");
        assert_eq!(back, vault);
        let old: Vault = serde_json::from_str(
            r#"{"version":1,"projects":[{"id":"x","name":"a","created_at":"2026-01-01T00:00:00Z","updated_at":"2026-01-01T00:00:00Z"}]}"#,
        )
        .expect("old vault");
        assert!(old.identity.is_none());
        assert!(old.projects[0].members.is_empty());
    }

    #[test]
    fn serde_defaults() {
        let v: Variable = serde_json::from_str(r#"{"key":"A","value":"1"}"#).expect("de");
        assert!(v.secret);
        assert!(v.comment.is_empty());
        let vault: Vault = serde_json::from_str(r#"{"version":1}"#).expect("de");
        assert_eq!(vault.settings.auto_lock_minutes, 30);
    }
}

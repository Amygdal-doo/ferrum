//! Profiles: identity + SSH key, passphrases in the OS keychain, key tools.

use crate::{AppError, AppResult, AppServices};
use ferrumgit_domain::{Profile, ProfileId, Secret, SshAuth};
use ferrumgit_git::{GitError, ssh_command};
use std::path::Path;

/// What to do with the stored passphrase when saving a profile.
#[derive(Debug, Clone)]
pub enum PassphraseChange {
    Keep,
    Set(Secret),
    Clear,
}

impl AppServices {
    /// Saves the profile and updates its keychain entry.
    pub async fn save_profile(
        &self,
        mut profile: Profile,
        change: PassphraseChange,
    ) -> AppResult<Profile> {
        let account = profile.id.to_string();
        let creds = self.creds.clone();
        let stored = match change {
            PassphraseChange::Keep => None,
            PassphraseChange::Set(secret) if !secret.is_empty() => Some(Some(secret)),
            PassphraseChange::Set(_) | PassphraseChange::Clear => Some(None),
        };
        if let Some(value) = stored {
            let has = value.is_some();
            tokio::task::spawn_blocking(move || match value {
                Some(s) => creds.set(&account, s.expose()),
                None => creds.delete(&account),
            })
            .await
            .map_err(|e| AppError::Credentials(e.to_string()))?
            .map_err(|e| AppError::Credentials(e.to_string()))?;
            if let SshAuth::KeyFile { has_passphrase, .. } = &mut profile.ssh {
                *has_passphrase = has;
            }
        }
        if matches!(profile.ssh, SshAuth::System) {
            self.delete_passphrase(profile.id).await?;
        }
        self.store.save_profile(&profile).await?;
        Ok(profile)
    }

    pub async fn delete_profile(&self, id: ProfileId) -> AppResult<()> {
        self.delete_passphrase(id).await?;
        self.store.delete_profile(id).await?;
        Ok(())
    }

    pub(crate) async fn passphrase(&self, id: ProfileId) -> AppResult<Option<Secret>> {
        let creds = self.creds.clone();
        let account = id.to_string();
        tokio::task::spawn_blocking(move || creds.get(&account))
            .await
            .map_err(|e| AppError::Credentials(e.to_string()))?
            .map(|o| o.map(Secret::new))
            .map_err(|e| AppError::Credentials(e.to_string()))
    }

    async fn delete_passphrase(&self, id: ProfileId) -> AppResult<()> {
        let creds = self.creds.clone();
        let account = id.to_string();
        tokio::task::spawn_blocking(move || creds.delete(&account))
            .await
            .map_err(|e| AppError::Credentials(e.to_string()))?
            .map_err(|e| AppError::Credentials(e.to_string()))
    }

    /// `ssh -T user@host` with the profile's key; trusts a new host key.
    pub async fn test_profile(&self, profile: &Profile, host: &str) -> AppResult<String> {
        let net = self.net_env(Some(profile)).await?;
        let (user, host) = host.trim().split_once('@').unwrap_or(("git", host.trim()));
        Ok(ferrumgit_git::test_connection(host, user, profile.key_path(), &net).await?)
    }

    /// Generates an ed25519 key pair (no passphrase) at `path`; returns the public key.
    pub async fn generate_key(&self, path: &str, comment: &str) -> AppResult<String> {
        let path = ferrumgit_git::expand_home(path);
        if path.is_empty() || Path::new(&path).exists() {
            return Err(AppError::Invalid(format!(
                "{path} already exists; choose a new file name"
            )));
        }
        if let Some(parent) = Path::new(&path).parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let out = tokio::process::Command::new("ssh-keygen")
            .args(["-q", "-t", "ed25519", "-N", "", "-C", comment, "-f", &path])
            .stdin(std::process::Stdio::null())
            .output()
            .await
            .map_err(|e| AppError::Git(GitError::Spawn(format!("ssh-keygen: {e}"))))?;
        if !out.status.success() {
            return Err(AppError::Invalid(
                String::from_utf8_lossy(&out.stderr).trim().to_owned(),
            ));
        }
        self.public_key(&path).await
    }

    /// Contents of `<key>.pub`, for copying into GitHub/GitLab settings.
    pub async fn public_key(&self, key_path: &str) -> AppResult<String> {
        let pub_path = format!("{}.pub", ferrumgit_git::expand_home(key_path));
        tokio::fs::read_to_string(&pub_path)
            .await
            .map(|s| s.trim().to_owned())
            .map_err(|_| AppError::Invalid(format!("no public key at {pub_path}")))
    }

    /// The `GIT_SSH_COMMAND` a profile uses (shown in the profile editor).
    #[must_use]
    pub fn ssh_command_preview(profile: &Profile) -> String {
        ssh_command(profile.key_path(), &[])
    }

    /// Private keys found in `~/.ssh` (files with a matching `.pub`).
    pub async fn discover_keys(&self) -> Vec<String> {
        let Some(home) = std::env::var_os("HOME") else {
            return Vec::new();
        };
        let dir = Path::new(&home).join(".ssh");
        let mut keys = Vec::new();
        let Ok(mut rd) = tokio::fs::read_dir(&dir).await else {
            return keys;
        };
        while let Ok(Some(e)) = rd.next_entry().await {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "pub") {
                let private = p.with_extension("");
                if private.is_file() {
                    keys.push(private.display().to_string());
                }
            }
        }
        keys.sort();
        keys
    }
}

//! Repository loading and operations, with the repo's profile applied.

use crate::{AppError, AppResult, AppServices};
use ferrumgit_domain::{
    CommitDetails, DiffSource, FileDiff, Oid, Profile, ProfileId, RepoEntry, RepoOp, Stash,
};
use ferrumgit_git::{CancellationToken, NetEnv, OpContext, Snapshot};
use ferrumgit_graph::{GraphRow, layout};
use std::path::Path;

/// A loaded repository: git snapshot plus graph layout.
#[derive(Debug, Clone, Default)]
pub struct RepoData {
    pub snapshot: Snapshot,
    pub graph: Vec<GraphRow>,
}

impl AppServices {
    /// Validates `path`, normalises it to the working-tree root and records it
    /// as recently opened. An existing entry for the same root keeps its id and profile.
    pub async fn add_repo(&self, path: &str) -> AppResult<RepoEntry> {
        let repo = ferrumgit_git::Repo::open(&self.git, Path::new(path)).await?;
        let root = repo.root.display().to_string();
        let existing = self
            .store
            .list_repos()
            .await?
            .into_iter()
            .find(|r| r.path == root);
        let mut entry = existing.unwrap_or_else(|| RepoEntry::new(root));
        entry.last_opened_ms = chrono::Utc::now().timestamp_millis();
        self.store.save_repo(&entry).await?;
        Ok(entry)
    }

    pub async fn load(&self, path: &str, limit: usize) -> AppResult<RepoData> {
        let repo = self.repo(path).await?;
        let snapshot = repo.snapshot(limit).await?;
        let graph = layout(&snapshot.commits);
        Ok(RepoData { snapshot, graph })
    }

    pub async fn fingerprint(&self, path: &str) -> AppResult<String> {
        Ok(self.repo(path).await?.fingerprint().await?)
    }

    /// Diff of one working-copy file. `None` when there is nothing to show.
    pub async fn file_diff(
        &self,
        path: &str,
        file: &str,
        orig: Option<&str>,
        source: &DiffSource,
    ) -> AppResult<Option<FileDiff>> {
        let repo = self.repo(path).await?;
        Ok(match source {
            DiffSource::Unstaged => repo.diff_unstaged(file).await?,
            DiffSource::Staged => repo.diff_staged(file, orig).await?,
            DiffSource::Untracked => repo.diff_untracked(file).await?,
            DiffSource::Commit => {
                return Err(AppError::Invalid(
                    "commit diffs come with commit details".into(),
                ));
            }
        })
    }

    pub async fn commit_details(&self, path: &str, oid: &Oid) -> AppResult<CommitDetails> {
        Ok(self.repo(path).await?.commit_details(oid.as_str()).await?)
    }

    pub async fn stash_details(&self, path: &str, stash: &Stash) -> AppResult<CommitDetails> {
        Ok(self.repo(path).await?.stash_details(stash).await?)
    }

    pub async fn head_message(&self, path: &str) -> AppResult<String> {
        Ok(self.repo(path).await?.head_message().await?)
    }

    /// SSH environment for `profile` (`None` = system ssh), with its stored passphrase.
    pub async fn net_env(&self, profile: Option<&Profile>) -> AppResult<NetEnv> {
        let key = profile.and_then(Profile::key_path).map(str::to_owned);
        let passphrase = match profile {
            Some(p)
                if matches!(
                    p.ssh,
                    ferrumgit_domain::SshAuth::KeyFile {
                        has_passphrase: true,
                        ..
                    }
                ) =>
            {
                self.passphrase(p.id).await?
            }
            _ => None,
        };
        let askpass = self.git.askpass().map(Path::to_path_buf);
        Ok(NetEnv::new(
            key.as_deref(),
            passphrase.as_ref().map(|s| s.expose()),
            askpass.as_deref(),
        )?)
    }

    /// Runs `op` in `entry`'s repository using its profile's identity and SSH key.
    pub async fn run_op(
        &self,
        entry: &RepoEntry,
        profile: Option<&Profile>,
        op: &RepoOp,
        cancel: Option<CancellationToken>,
    ) -> AppResult<Option<String>> {
        let repo = self.repo(&entry.path).await?;
        let net = if op.is_network() {
            Some(self.net_env(profile).await?)
        } else {
            None
        };
        let identity = profile.and_then(Profile::identity);
        tracing::info!(repo = %entry.id, op = %op.label(), network = op.is_network(), "running operation");
        let result = repo
            .execute(
                op,
                OpContext {
                    identity,
                    net,
                    cancel,
                },
            )
            .await;
        result.map_err(|e| {
            if op.is_network() {
                e.with_ssh_hint()
            } else {
                e
            }
            .into()
        })
    }

    /// Clones into `dest` with `profile`'s SSH key and records the repository with that profile.
    pub async fn clone_repo(
        &self,
        url: &str,
        dest: &str,
        profile: Option<Profile>,
        cancel: Option<CancellationToken>,
    ) -> AppResult<RepoEntry> {
        let net = self.net_env(profile.as_ref()).await?;
        let repo = self
            .git
            .clone_repo(url, Path::new(dest), Some(net), cancel)
            .await?;
        let mut entry = self.add_repo(&repo.root.display().to_string()).await?;
        if let Some(p) = profile {
            entry.profile = Some(p.id);
            self.store.save_repo(&entry).await?;
        }
        Ok(entry)
    }

    pub async fn init_repo(&self, dir: &str) -> AppResult<RepoEntry> {
        let repo = self.git.init_repo(Path::new(dir), "main").await?;
        self.add_repo(&repo.root.display().to_string()).await
    }

    /// Assigns (or clears) the profile used by a repository.
    pub async fn set_repo_profile(
        &self,
        mut entry: RepoEntry,
        profile: Option<ProfileId>,
    ) -> AppResult<RepoEntry> {
        entry.profile = profile;
        self.store.save_repo(&entry).await?;
        Ok(entry)
    }
}

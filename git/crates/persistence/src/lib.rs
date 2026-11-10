//! Local application state (repositories, profiles, workspace, settings) in a
//! dedicated SQLite file. Records are JSON documents; unreadable records are
//! logged (ids only) and skipped instead of failing the whole load.

use chrono::Utc;
use ferrumgit_domain::{
    AppSettings, ForgeHost, Profile, ProfileId, RepoEntry, RepoId, WorkspaceState,
};
use serde::Serialize;
use serde::de::DeserializeOwned;
use sqlx::SqlitePool;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use std::path::Path;

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("local database error: {0}")]
    Db(#[from] sqlx::Error),
    #[error("local database migration failed: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    #[error("corrupt local record: {0}")]
    Corrupt(String),
}

pub type StoreResult<T> = Result<T, StoreError>;

const SETTINGS_KEY: &str = "app";
const WORKSPACE_KEY: &str = "workspace";
/// Per-host forge token source (never the token itself).
const FORGE_HOSTS_KEY: &str = "forge_hosts";

/// Handle to the app-state database. Cheap to clone.
#[derive(Debug, Clone)]
pub struct Store {
    pool: SqlitePool,
}

impl Store {
    pub async fn open(path: &Path) -> StoreResult<Self> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .journal_mode(SqliteJournalMode::Wal)
            .foreign_keys(true);
        Self::from_options(options).await
    }

    pub async fn in_memory() -> StoreResult<Self> {
        Self::from_options(SqliteConnectOptions::new().in_memory(true)).await
    }

    async fn from_options(options: SqliteConnectOptions) -> StoreResult<Self> {
        // One connection: keeps in-memory databases coherent and writes serialized.
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(options)
            .await?;
        sqlx::migrate!("./migrations").run(&pool).await?;
        Ok(Self { pool })
    }

    // Repositories

    /// Most recently opened first.
    pub async fn list_repos(&self) -> StoreResult<Vec<RepoEntry>> {
        let mut repos: Vec<RepoEntry> = self.list_docs("SELECT id, data FROM repos").await?;
        repos.sort_by_key(|r| std::cmp::Reverse(r.last_opened_ms));
        Ok(repos)
    }

    /// Inserts or updates by id. A different entry with the same path is replaced.
    pub async fn save_repo(&self, repo: &RepoEntry) -> StoreResult<()> {
        sqlx::query("DELETE FROM repos WHERE path = ? AND id <> ?")
            .bind(&repo.path)
            .bind(repo.id.to_string())
            .execute(&self.pool)
            .await?;
        sqlx::query(
            "INSERT INTO repos (id, path, data, updated_at) VALUES (?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET path = excluded.path, data = excluded.data, updated_at = excluded.updated_at",
        )
        .bind(repo.id.to_string())
        .bind(&repo.path)
        .bind(to_json(repo)?)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    pub async fn delete_repo(&self, id: RepoId) -> StoreResult<()> {
        sqlx::query("DELETE FROM repos WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    // Profiles

    /// Sorted by name.
    pub async fn list_profiles(&self) -> StoreResult<Vec<Profile>> {
        let mut p: Vec<Profile> = self.list_docs("SELECT id, data FROM profiles").await?;
        p.sort_by_key(|p| p.name.to_lowercase());
        Ok(p)
    }

    pub async fn save_profile(&self, profile: &Profile) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO profiles (id, data, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET data = excluded.data, updated_at = excluded.updated_at",
        )
        .bind(profile.id.to_string())
        .bind(to_json(profile)?)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Deletes the profile and unassigns it from repositories.
    pub async fn delete_profile(&self, id: ProfileId) -> StoreResult<()> {
        sqlx::query("DELETE FROM profiles WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        for mut repo in self.list_repos().await? {
            if repo.profile == Some(id) {
                repo.profile = None;
                self.save_repo(&repo).await?;
            }
        }
        Ok(())
    }

    // Settings & workspace

    pub async fn load_settings(&self) -> StoreResult<AppSettings> {
        self.load_key(SETTINGS_KEY).await
    }

    pub async fn save_settings(&self, s: &AppSettings) -> StoreResult<()> {
        self.save_key(SETTINGS_KEY, s).await
    }

    pub async fn load_workspace(&self) -> StoreResult<WorkspaceState> {
        self.load_key(WORKSPACE_KEY).await
    }

    pub async fn save_workspace(&self, ws: &WorkspaceState) -> StoreResult<()> {
        self.save_key(WORKSPACE_KEY, ws).await
    }

    pub async fn load_forge_hosts(&self) -> StoreResult<Vec<ForgeHost>> {
        self.load_key(FORGE_HOSTS_KEY).await
    }

    pub async fn save_forge_hosts(&self, hosts: &[ForgeHost]) -> StoreResult<()> {
        self.save_key(FORGE_HOSTS_KEY, &hosts).await
    }

    async fn list_docs<T: DeserializeOwned>(&self, sql: &'static str) -> StoreResult<Vec<T>> {
        let rows: Vec<(String, String)> = sqlx::query_as(sql).fetch_all(&self.pool).await?;
        Ok(rows
            .into_iter()
            .filter_map(|(id, json)| match serde_json::from_str(&json) {
                Ok(v) => Some(v),
                Err(e) => {
                    tracing::warn!(id, kind = ?e.classify(), "skipping unreadable record");
                    None
                }
            })
            .collect())
    }

    async fn load_key<T: DeserializeOwned + Default>(&self, key: &str) -> StoreResult<T> {
        let json: Option<String> = sqlx::query_scalar("SELECT value FROM settings WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        let Some(json) = json else {
            return Ok(T::default());
        };
        Ok(serde_json::from_str(&json).unwrap_or_else(|e| {
            tracing::warn!(key, kind = ?e.classify(), "stored value unreadable; using defaults");
            T::default()
        }))
    }

    async fn save_key<T: Serialize>(&self, key: &str, value: &T) -> StoreResult<()> {
        sqlx::query(
            "INSERT INTO settings (key, value, updated_at) VALUES (?, ?, ?)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_at = excluded.updated_at",
        )
        .bind(key)
        .bind(to_json(value)?)
        .bind(now())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    #[cfg(test)]
    async fn exec(&self, sql: &'static str) {
        sqlx::query(sql).execute(&self.pool).await.expect("raw sql");
    }
}

fn now() -> String {
    Utc::now().to_rfc3339()
}

fn to_json<T: Serialize>(value: &T) -> StoreResult<String> {
    serde_json::to_string(value)
        .map_err(|e| StoreError::Corrupt(format!("unserializable value ({:?})", e.classify())))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumgit_domain::{PullMode, SshAuth, Theme};

    async fn mem() -> Store {
        Store::in_memory().await.expect("store")
    }

    #[tokio::test]
    async fn repos_round_trip_sorted_and_unique_by_path() {
        let s = mem().await;
        let mut a = RepoEntry::new("/code/a");
        a.last_opened_ms = 1;
        let mut b = RepoEntry::new("/code/b");
        b.last_opened_ms = 2;
        s.save_repo(&a).await.expect("a");
        s.save_repo(&b).await.expect("b");
        let list = s.list_repos().await.expect("list");
        assert_eq!(
            list.iter().map(|r| r.name.as_str()).collect::<Vec<_>>(),
            ["b", "a"]
        );

        // Re-adding the same path under a new id replaces the old entry.
        let again = RepoEntry::new("/code/a");
        s.save_repo(&again).await.expect("again");
        let list = s.list_repos().await.expect("list");
        assert_eq!(list.len(), 2);
        assert!(list.iter().any(|r| r.id == again.id));

        s.delete_repo(b.id).await.expect("delete");
        assert_eq!(s.list_repos().await.expect("list").len(), 1);
    }

    #[tokio::test]
    async fn deleting_profile_unassigns_repos() {
        let s = mem().await;
        let mut p = Profile::new("Work");
        p.ssh = SshAuth::KeyFile {
            path: "~/.ssh/work".into(),
            has_passphrase: true,
        };
        s.save_profile(&p).await.expect("profile");
        let mut r = RepoEntry::new("/code/w");
        r.profile = Some(p.id);
        s.save_repo(&r).await.expect("repo");
        assert_eq!(s.list_profiles().await.expect("list"), vec![p.clone()]);

        s.delete_profile(p.id).await.expect("delete");
        assert!(s.list_profiles().await.expect("list").is_empty());
        assert_eq!(s.list_repos().await.expect("repos")[0].profile, None);
    }

    #[tokio::test]
    async fn settings_and_workspace_defaults_and_corruption() {
        let s = mem().await;
        assert_eq!(
            s.load_settings().await.expect("defaults"),
            AppSettings::default()
        );
        let settings = AppSettings {
            theme: Theme::Dark,
            pull_mode: PullMode::Rebase,
            side_by_side: true,
            ..AppSettings::default()
        };
        s.save_settings(&settings).await.expect("save");
        assert_eq!(s.load_settings().await.expect("load"), settings);
        // Settings saved before a field existed load with its default.
        s.exec(r#"UPDATE settings SET value = '{"theme":"dark"}' WHERE key = 'app'"#)
            .await;
        assert!(!s.load_settings().await.expect("old").side_by_side);

        assert!(s.load_forge_hosts().await.expect("hosts").is_empty());
        let hosts = vec![ferrumgit_domain::ForgeHost {
            host: "github.com".into(),
            token_source: ferrumgit_domain::TokenSource::GhCli,
        }];
        s.save_forge_hosts(&hosts).await.expect("save hosts");
        assert_eq!(s.load_forge_hosts().await.expect("hosts"), hosts);

        let ws = WorkspaceState {
            open: vec![RepoId::new()],
            active: None,
        };
        s.save_workspace(&ws).await.expect("save ws");
        assert_eq!(s.load_workspace().await.expect("ws"), ws);

        s.exec("UPDATE settings SET value = '{not json' WHERE key = 'app'")
            .await;
        assert_eq!(
            s.load_settings().await.expect("fallback"),
            AppSettings::default()
        );
        s.exec("INSERT INTO profiles (id, data, updated_at) VALUES ('x', 'garbage', '')")
            .await;
        assert!(s.list_profiles().await.expect("skip corrupt").is_empty());
    }
}

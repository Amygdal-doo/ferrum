//! FerrumLauncher core: which Ferrum apps exist, which versions are
//! published as GitHub Releases, which are installed locally, and the
//! one-time move from Ferrum Toolbox (the launcher's former name).
//!
//! No UI code lives here so everything is unit-testable; the desktop app
//! drives these APIs from a tokio runtime.

pub mod auth;
pub mod auto_update;
pub mod catalog;
pub mod github;
pub mod install;
pub mod migrate;
pub mod platform;
pub mod release;
pub mod self_rename;
pub mod self_update;
pub mod shared_prefs;
pub mod shell;
pub mod store;
pub mod synced;
pub mod uninstall;
pub mod usage;

pub use auth::{GitHubToken, TokenSource};
pub use catalog::{AppId, AppInfo, AppStatus, Catalog, CatalogEntry};
pub use github::{DownloadProgress, GitHubClient};
pub use platform::Platform;
pub use release::{Asset, Channel, ReleaseInfo, UpdateChannel, current_target};
pub use store::{AppPrefs, InstalledVersion, Paths, Settings, State};
pub use synced::SyncedSettings;

#[derive(Debug, thiserror::Error)]
pub enum LauncherError {
    #[error("GitHub rejected the token (401). Check it in Settings.")]
    Unauthorized,
    #[error("not found on GitHub (404): {0}. If the repo is private, add a token in Settings.")]
    NotFound(String),
    #[error("GitHub API rate limit reached{}. Add a token in Settings to raise the limit.", .reset.map(|t| format!("; resets at {}", t.format("%H:%M"))).unwrap_or_default())]
    RateLimited {
        reset: Option<chrono::DateTime<chrono::Local>>,
    },
    #[error("GitHub returned HTTP {status}: {message}")]
    Http { status: u16, message: String },
    #[error("network error: {0}")]
    Network(String),
    #[error("checksum mismatch for {asset}: expected {expected}, got {actual}")]
    ChecksumMismatch {
        asset: String,
        expected: String,
        actual: String,
    },
    #[error("invalid checksum file for {0}")]
    InvalidChecksum(String),
    #[error("archive entry escapes the install directory: {0}")]
    UnsafeArchiveEntry(String),
    #[error("archive does not contain the `{0}` binary at its root")]
    MissingBinary(String),
    #[error("no build of {app} {version} for {target}")]
    NoAssetForTarget {
        app: String,
        version: String,
        target: String,
    },
    #[error("this platform has no published builds")]
    UnsupportedPlatform,
    #[error("{0}")]
    Invalid(String),
    #[error("download cancelled")]
    Cancelled,
    #[error("credential store: {0}")]
    Credentials(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
    #[error("state file is corrupt: {0}")]
    Corrupt(#[from] serde_json::Error),
}

pub type Result<T, E = LauncherError> = std::result::Result<T, E>;

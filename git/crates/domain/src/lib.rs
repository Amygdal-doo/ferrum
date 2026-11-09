//! Pure FerrumGIT domain types. No UI, no processes, no database.
//!
//! Persisted types are serde (JSON) with `#[serde(default)]` on structs so
//! older records keep loading when fields are added.

mod commit;
mod diff;
mod history;
mod ids;
mod ops;
mod profile;
mod refs;
mod repo;
pub mod search;
mod secret;
mod status;
pub mod text;
mod undo;
mod workspace;

pub use commit::{Commit, CommitDetails, Oid, Stash};
pub use diff::{DiffLine, DiffSource, FileDiff, Hunk, LineKind, SplitRow};
pub use history::{Blame, BlameCommit, BlameLine, FileCommit};
pub use ids::{ProfileId, RepoId};
pub use ops::{PullMode, RepoOp, ResetMode};
pub use profile::{Profile, SshAuth};
pub use refs::{GitRef, RefKind, Remote, is_ssh_url, ssh_host};
pub use repo::RepoEntry;
pub use secret::Secret;
pub use status::{BranchInfo, ChangeKind, FileStatus, RepoOperation, WorkingStatus};
pub use undo::Undo;
pub use workspace::{AppSettings, Theme, WorkspaceState};

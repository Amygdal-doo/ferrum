//! Git execution for FerrumGIT.
//!
//! Like Tower, FerrumGIT drives the system `git` binary instead of
//! linking libgit2: users get their own config, hooks, signing, credential
//! helpers, `~/.ssh/config` and ssh-agent for free. Output is read in
//! machine formats (`--porcelain=v2 -z`, `--format` with unit separators) and
//! parsed by the pure functions in [`parse`].
//!
//! Network commands take a [`NetEnv`] built from the repository's profile so
//! each project can push/pull with its own SSH key.

mod conflict;
mod error;
mod history;
pub mod parse;
pub mod patch;
mod pr;
mod read;
mod rebase;
mod runner;
mod ssh;
mod write;

pub use error::{GitError, GitResult};
pub use pr::PR_NAMESPACE;
pub use read::{MAX_PATCH_BYTES, Repo, Snapshot};
pub use rebase::{MAX_REBASE_STEPS, shell_quote};
pub use runner::{Cmd, Git, Output};
pub use ssh::{
    ASKPASS_ENV, ASKPASS_FILE_ENV, NetEnv, askpass_main, expand_home, ssh_command, test_connection,
};
pub use tokio_util::sync::CancellationToken;
pub use write::OpContext;

//! Pure parsers for git's machine-readable output. No processes here.

mod diff;
mod history;
mod log;
mod refs;
mod status;

pub use diff::{parse_diff, unquote};
pub use history::{FILE_LOG_FORMAT, REFLOG_FORMAT, parse_blame, parse_file_log, parse_reflog};
pub use log::{
    COMMIT_FORMAT, DETAILS_FORMAT, STASH_FORMAT, parse_details_header, parse_log, parse_stashes,
};
pub use refs::{REFS_FORMAT, parse_refs, parse_remotes};
pub use status::parse_status;

/// Field separator used in `--format` strings (`%x1f`).
pub const FIELD: char = '\u{1f}';

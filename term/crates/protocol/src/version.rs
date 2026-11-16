//! Which build a server is, and whether it is older than the client talking
//! to it.
//!
//! The server keeps running across app updates (like tmux), so a freshly
//! updated FerrumTerm can find a server started by last week's build. Two
//! numbers tell builds apart:
//!
//! - [`build_version`]: the release version (`FERRUM_VERSION` at build time,
//!   e.g. `0.12.0-alpha.1`; the crate version in dev builds).
//! - [`SERVER_REVISION`]: what clients may rely on the server doing.
//!   **Bump it whenever server behaviour a client depends on changes** — a new
//!   request, event or field the UI needs to work properly, or detection /
//!   notification logic whose absence makes a feature silently not work.
//!   Pure bug fixes and internal changes do not need a bump. (Incompatible wire
//!   changes bump [`PROTOCOL_VERSION`](crate::PROTOCOL_VERSION) instead.)
//!
//! Servers from before this existed send neither field; they decode as
//! revision 0 / unknown version, i.e. older than any current client.

use serde::{Deserialize, Serialize};
use std::cmp::Ordering;

/// Feature revision of the server code in this build (see the module docs
/// for when to bump it).
///
/// History:
/// - 0: servers that do not report a revision (before 0.12).
/// - 1: agent detection in the sidebar, agent notifications, build reporting.
pub const SERVER_REVISION: u32 = 1;

/// The release version of this build: `FERRUM_VERSION` at compile time
/// (set by the release scripts), else the crate version.
#[must_use]
pub const fn build_version() -> &'static str {
    match option_env!("FERRUM_VERSION") {
        Some(v) => v,
        None => env!("CARGO_PKG_VERSION"),
    }
}

/// How a server compares with the client (app or `fterm`) talking to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServerAge {
    /// Same revision and same (or unknown) release.
    Current,
    /// Lower revision, or one that does not report it: features the client
    /// relies on are missing until the server restarts.
    Older,
    /// Same revision but an older release: nothing the client needs is
    /// missing, but a restart picks up the server's own fixes.
    OlderBuild,
    /// The server is newer than the client.
    Newer,
}

impl ServerAge {
    /// Whether to tell the user to restart the server now.
    #[must_use]
    pub fn needs_restart(self) -> bool {
        self == Self::Older
    }
}

/// Compares a server's `(version, revision)` with the client's.
///
/// The revision decides; with equal revisions an older release version (by
/// semver precedence) is [`ServerAge::OlderBuild`]. Versions that are empty
/// or not semver compare as equal.
#[must_use]
pub fn compare(
    server_version: &str,
    server_revision: u32,
    client_version: &str,
    client_revision: u32,
) -> ServerAge {
    match server_revision.cmp(&client_revision) {
        Ordering::Less => ServerAge::Older,
        Ordering::Greater => ServerAge::Newer,
        Ordering::Equal => match (parse(server_version), parse(client_version)) {
            (Some(s), Some(c)) => match s.cmp_precedence(&c) {
                Ordering::Less => ServerAge::OlderBuild,
                Ordering::Greater => ServerAge::Newer,
                Ordering::Equal => ServerAge::Current,
            },
            _ => ServerAge::Current,
        },
    }
}

/// [`compare`] against this build.
#[must_use]
pub fn compare_with_this_build(server_version: &str, server_revision: u32) -> ServerAge {
    compare(
        server_version,
        server_revision,
        build_version(),
        SERVER_REVISION,
    )
}

fn parse(v: &str) -> Option<semver::Version> {
    semver::Version::parse(v.trim().trim_start_matches('v')).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lower_revision_is_older_whatever_the_version() {
        assert_eq!(compare("0.12.0", 0, "0.12.0", 1), ServerAge::Older);
        assert_eq!(compare("0.13.0", 1, "0.12.0", 2), ServerAge::Older);
        assert!(compare("", 0, "0.12.0", 1).needs_restart());
    }

    #[test]
    fn a_higher_revision_is_newer() {
        assert_eq!(compare("0.11.0", 3, "0.12.0", 2), ServerAge::Newer);
        assert!(!ServerAge::Newer.needs_restart());
    }

    #[test]
    fn equal_revisions_compare_releases() {
        assert_eq!(compare("0.12.0", 1, "0.12.0", 1), ServerAge::Current);
        assert_eq!(compare("0.11.2", 1, "0.12.0", 1), ServerAge::OlderBuild);
        assert_eq!(compare("0.12.0", 1, "0.11.2", 1), ServerAge::Newer);
        // Pre-releases come before their release.
        assert_eq!(
            compare("0.12.0-alpha.1", 1, "0.12.0", 1),
            ServerAge::OlderBuild
        );
        assert_eq!(
            compare("0.12.0-alpha.1", 1, "0.12.0-alpha.2", 1),
            ServerAge::OlderBuild
        );
        assert_eq!(compare("v0.12.0", 1, "0.12.0", 1), ServerAge::Current);
        // A different build of the same revision does not demand a restart.
        assert!(!ServerAge::OlderBuild.needs_restart());
    }

    #[test]
    fn unknown_versions_fall_back_to_the_revision() {
        assert_eq!(compare("", 1, "0.12.0", 1), ServerAge::Current);
        assert_eq!(compare("dev", 1, "0.12.0", 1), ServerAge::Current);
    }

    #[test]
    fn this_build_is_current_with_itself() {
        assert_eq!(
            compare_with_this_build(build_version(), SERVER_REVISION),
            ServerAge::Current
        );
        assert!(!build_version().is_empty());
    }
}

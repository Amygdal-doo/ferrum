//! Branches, tags and remotes.

use crate::commit::Oid;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum RefKind {
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GitRef {
    /// Short name: `main`, `origin/main`, `v1.0`.
    pub name: String,
    /// Full name: `refs/heads/main`.
    pub full_name: String,
    /// Commit the ref points at (peeled for annotated tags).
    pub target: Oid,
    pub kind: Option<RefKind>,
    /// Upstream short name for local branches (`origin/main`).
    pub upstream: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// The upstream is configured but the remote branch no longer exists.
    pub upstream_gone: bool,
    /// The currently checked-out branch.
    pub is_head: bool,
    /// Local branch fully merged into HEAD (safe to delete).
    pub merged: bool,
}

impl GitRef {
    #[must_use]
    pub fn is(&self, kind: RefKind) -> bool {
        self.kind == Some(kind)
    }

    /// For a remote branch `origin/feature/x`: `("origin", "feature/x")`.
    #[must_use]
    pub fn remote_parts(&self) -> Option<(&str, &str)> {
        if !self.is(RefKind::RemoteBranch) {
            return None;
        }
        self.name.split_once('/')
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Remote {
    pub name: String,
    pub fetch_url: String,
    pub push_url: String,
}

impl Remote {
    /// SSH remotes (`git@host:…`, `ssh://…`) use the repository's profile key.
    #[must_use]
    pub fn is_ssh(&self) -> bool {
        is_ssh_url(&self.fetch_url) || is_ssh_url(&self.push_url)
    }
}

/// `git@github.com:o/r.git`, `ssh://git@host/r`, `user@host:path`.
#[must_use]
pub fn is_ssh_url(url: &str) -> bool {
    let u = url.trim();
    if u.starts_with("ssh://") || u.starts_with("git+ssh://") {
        return true;
    }
    if u.contains("://") {
        return false;
    }
    // scp-like syntax: [user@]host:path, with no slash before the colon.
    match u.find(':') {
        Some(i) => !u[..i].contains('/') && i > 0,
        None => false,
    }
}

/// Host part of an SSH URL, for "Test connection" defaults.
#[must_use]
pub fn ssh_host(url: &str) -> Option<String> {
    let u = url.trim();
    let rest = u
        .strip_prefix("ssh://")
        .or_else(|| u.strip_prefix("git+ssh://"));
    let hostpart = match rest {
        Some(r) => r.split('/').next()?,
        None if is_ssh_url(u) => u.split(':').next()?,
        None => return None,
    };
    let host = hostpart.rsplit('@').next()?;
    let host = host.split(':').next()?;
    (!host.is_empty()).then(|| host.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_ssh_urls() {
        assert!(is_ssh_url("git@github.com:o/r.git"));
        assert!(is_ssh_url("ssh://git@host:2222/r.git"));
        assert!(!is_ssh_url("https://github.com/o/r.git"));
        assert!(!is_ssh_url("/local/path"));
        assert!(!is_ssh_url("./a:b"));
    }

    #[test]
    fn extracts_host() {
        assert_eq!(
            ssh_host("git@github.com:o/r.git").as_deref(),
            Some("github.com")
        );
        assert_eq!(
            ssh_host("ssh://git@gitlab.example.com:2222/r").as_deref(),
            Some("gitlab.example.com")
        );
        assert_eq!(ssh_host("https://github.com/o/r"), None);
    }
}

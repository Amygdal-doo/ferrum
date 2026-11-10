//! A repository bookmarked in FerrumGIT.

use crate::forge::RepoForge;
use crate::ids::{ProfileId, RepoId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct RepoEntry {
    pub id: RepoId,
    /// Absolute path of the working tree.
    pub path: String,
    pub name: String,
    /// Profile used for commits and SSH; `None` = system git/ssh configuration.
    pub profile: Option<ProfileId>,
    /// Unix millis; used to sort the recent list.
    pub last_opened_ms: i64,
    /// Pull request integration overrides (remote, forge type, API URL).
    pub forge: RepoForge,
}

impl RepoEntry {
    #[must_use]
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let name = path
            .trim_end_matches(['/', '\\'])
            .rsplit(['/', '\\'])
            .next()
            .unwrap_or(&path)
            .to_owned();
        Self {
            id: RepoId::new(),
            path,
            name,
            profile: None,
            last_opened_ms: 0,
            forge: RepoForge::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::RepoEntry;

    #[test]
    fn name_is_last_path_segment() {
        assert_eq!(RepoEntry::new("/Users/me/code/ferrum/").name, "ferrum");
        assert_eq!(RepoEntry::new("C:\\src\\app").name, "app");
    }
}

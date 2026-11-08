//! The Ferrum apps the toolbox manages.

use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Stable id used in release tags (`<id>-v<semver>`), asset names and paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppId {
    Ide,
    Api,
    Git,
    Data,
    Term,
    Toolbox,
}

impl AppId {
    pub const ALL: [AppId; 6] = [
        AppId::Ide,
        AppId::Api,
        AppId::Git,
        AppId::Data,
        AppId::Term,
        AppId::Toolbox,
    ];

    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            AppId::Ide => "ide",
            AppId::Api => "api",
            AppId::Git => "git",
            AppId::Data => "data",
            AppId::Term => "term",
            AppId::Toolbox => "toolbox",
        }
    }

    #[must_use]
    pub fn info(self) -> &'static AppInfo {
        CATALOG
            .iter()
            .find(|info| info.id == self)
            .unwrap_or(&CATALOG[0])
    }
}

impl fmt::Display for AppId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for AppId {
    type Err = ();

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        AppId::ALL.into_iter().find(|id| id.as_str() == s).ok_or(())
    }
}

#[derive(Debug)]
pub struct AppInfo {
    pub id: AppId,
    pub name: &'static str,
    pub description: &'static str,
    /// Executable at the root of the release archive.
    pub binary: &'static str,
    /// Name of a Zed `IconName` variant that fits the app.
    pub icon: &'static str,
}

pub static CATALOG: [AppInfo; 6] = [
    AppInfo {
        id: AppId::Ide,
        name: "FerrumIDE",
        description: "Code editor and IDE built on the Zed codebase.",
        binary: "zed",
        icon: "Code",
    },
    AppInfo {
        id: AppId::Api,
        name: "FerrumAPI",
        description: "HTTP/API client: collections, environments, history and code generation.",
        binary: "ferrumapi",
        icon: "Public",
    },
    AppInfo {
        id: AppId::Git,
        name: "FerrumGIT",
        description: "Git client with commit graph, staging, diffs and profiles.",
        binary: "ferrumgit",
        icon: "GitBranch",
    },
    AppInfo {
        id: AppId::Data,
        name: "FerrumData",
        description: "Database IDE: connections, SQL editor and result grid.",
        binary: "ferrumdata",
        icon: "DatabaseZap",
    },
    AppInfo {
        id: AppId::Term,
        name: "FerrumTerm",
        description: "GPU terminal with sessions, windows and panes.",
        binary: "ferrumterm",
        icon: "Terminal",
    },
    AppInfo {
        id: AppId::Toolbox,
        name: "FerrumToolbox",
        description: "Installs, updates and launches the Ferrum apps.",
        binary: "ferrum-toolbox",
        icon: "Box",
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_catalog_is_complete() {
        for id in AppId::ALL {
            assert_eq!(id.as_str().parse::<AppId>(), Ok(id));
            assert_eq!(id.info().id, id);
        }
        assert!("zed".parse::<AppId>().is_err());
    }
}

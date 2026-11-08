//! The Ferrum apps the toolbox manages, plus their development status from
//! `catalog.json` (fetched from the release repo, with an embedded fallback).

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
    /// Main executable: at the root of a plain archive, or in
    /// `<bundle_name>.app/Contents/MacOS/` of a macOS bundle archive.
    pub binary: &'static str,
    /// Other executables shipped next to `binary` that get CLI links too.
    pub extra_binaries: &'static [&'static str],
    /// macOS bundle name without `.app`.
    pub bundle_name: &'static str,
    /// Name of a Zed `IconName` variant that fits the app.
    pub icon: &'static str,
}

pub static CATALOG: [AppInfo; 6] = [
    AppInfo {
        id: AppId::Ide,
        name: "FerrumIDE",
        description: "Code editor and IDE built on the Zed codebase.",
        binary: "zed",
        extra_binaries: &[],
        bundle_name: "Ferrum IDE",
        icon: "Code",
    },
    AppInfo {
        id: AppId::Api,
        name: "FerrumAPI",
        description: "HTTP/API client: collections, environments, history and code generation.",
        binary: "ferrumapi",
        extra_binaries: &[],
        bundle_name: "FerrumAPI",
        icon: "Public",
    },
    AppInfo {
        id: AppId::Git,
        name: "FerrumGIT",
        description: "Git client with commit graph, staging, diffs and profiles.",
        binary: "ferrumgit",
        extra_binaries: &[],
        bundle_name: "FerrumGIT",
        icon: "GitBranch",
    },
    AppInfo {
        id: AppId::Data,
        name: "FerrumData",
        description: "Database IDE: connections, SQL editor and result grid.",
        binary: "ferrumdata",
        extra_binaries: &[],
        bundle_name: "FerrumData",
        icon: "DatabaseZap",
    },
    AppInfo {
        id: AppId::Term,
        name: "FerrumTerm",
        description: "GPU terminal with sessions, windows and panes.",
        binary: "ferrumterm",
        extra_binaries: &["fterm"],
        bundle_name: "FerrumTerm",
        icon: "Terminal",
    },
    AppInfo {
        id: AppId::Toolbox,
        name: "FerrumToolbox",
        description: "Installs, updates and launches the Ferrum apps.",
        binary: "ferrum-toolbox",
        extra_binaries: &[],
        bundle_name: "Ferrum Toolbox",
        icon: "Box",
    },
];

/// Where an app is in its life cycle, as set in `catalog.json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AppStatus {
    Planned,
    InDevelopment,
    Alpha,
    Beta,
    Stable,
    Deprecated,
}

impl AppStatus {
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            AppStatus::Planned => "Planned",
            AppStatus::InDevelopment => "In development",
            AppStatus::Alpha => "Alpha",
            AppStatus::Beta => "Beta",
            AppStatus::Stable => "Stable",
            AppStatus::Deprecated => "Deprecated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogEntry {
    pub id: AppId,
    pub status: AppStatus,
    pub summary: String,
    #[serde(default)]
    pub highlights: Vec<String>,
    #[serde(default)]
    pub roadmap: Vec<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

/// Parsed `catalog.json`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Catalog {
    pub apps: Vec<CatalogEntry>,
}

/// The copy of the repo's `catalog.json` this build was compiled with.
const EMBEDDED_CATALOG: &str = include_str!("../../../../catalog.json");

impl Catalog {
    /// Parses `catalog.json`. Entries for unknown app ids or with an unknown
    /// status are skipped (logged) so a newer catalog never breaks an older
    /// toolbox.
    pub fn parse(json: &str) -> crate::Result<Self> {
        #[derive(Deserialize)]
        struct RawCatalog {
            apps: Vec<serde_json::Value>,
        }
        let raw: RawCatalog = serde_json::from_str(json)?;
        let apps = raw
            .apps
            .into_iter()
            .filter_map(
                |value| match serde_json::from_value::<CatalogEntry>(value) {
                    Ok(entry) => Some(entry),
                    Err(e) => {
                        tracing::warn!(error = %e, "skipping catalog entry");
                        None
                    }
                },
            )
            .collect();
        Ok(Self { apps })
    }

    #[must_use]
    pub fn embedded() -> &'static Self {
        static EMBEDDED: std::sync::OnceLock<Catalog> = std::sync::OnceLock::new();
        EMBEDDED.get_or_init(|| {
            Self::parse(EMBEDDED_CATALOG).unwrap_or_else(|e| {
                tracing::error!(error = %e, "embedded catalog.json is invalid");
                Self::default()
            })
        })
    }

    #[must_use]
    pub fn entry(&self, app: AppId) -> Option<&CatalogEntry> {
        self.apps.iter().find(|entry| entry.id == app)
    }

    /// `self`, with apps it lacks filled in from `fallback`.
    #[must_use]
    pub fn or_fill_from(mut self, fallback: &Catalog) -> Self {
        for entry in &fallback.apps {
            if self.entry(entry.id).is_none() {
                self.apps.push(entry.clone());
            }
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedded_catalog_covers_every_app() {
        let catalog = Catalog::embedded();
        for app in AppId::ALL {
            let entry = catalog.entry(app).expect("catalog entry");
            assert!(!entry.summary.is_empty(), "{app} has no summary");
        }
    }

    #[test]
    fn parses_catalog_and_skips_unknown_entries() {
        let json = r#"{"version":1,"apps":[
          {"id":"data","status":"beta","summary":"DB IDE","highlights":["PG"],"homepage":"https://x"},
          {"id":"api","status":"in-development","summary":"API"},
          {"id":"rocket","status":"stable","summary":"Unknown app"},
          {"id":"git","status":"sideways","summary":"Unknown status"}
        ]}"#;
        let catalog = Catalog::parse(json).expect("parse");
        assert_eq!(catalog.apps.len(), 2);
        let data = catalog.entry(AppId::Data).expect("data");
        assert_eq!(data.status, AppStatus::Beta);
        assert_eq!(data.highlights, ["PG"]);
        assert_eq!(data.homepage.as_deref(), Some("https://x"));
        let api = catalog.entry(AppId::Api).expect("api");
        assert_eq!(api.status, AppStatus::InDevelopment);
        assert!(api.roadmap.is_empty());
        assert!(catalog.entry(AppId::Git).is_none());
        assert!(Catalog::parse("not json").is_err());
    }

    #[test]
    fn remote_catalog_falls_back_per_app() {
        let remote =
            Catalog::parse(r#"{"apps":[{"id":"data","status":"stable","summary":"Released"}]}"#)
                .expect("parse");
        let merged = remote.or_fill_from(Catalog::embedded());
        assert_eq!(
            merged.entry(AppId::Data).map(|e| e.status),
            Some(AppStatus::Stable)
        );
        assert!(AppId::ALL.iter().all(|app| merged.entry(*app).is_some()));
        assert_eq!(AppStatus::InDevelopment.label(), "In development");
    }

    #[test]
    fn ids_round_trip_and_catalog_is_complete() {
        for id in AppId::ALL {
            assert_eq!(id.as_str().parse::<AppId>(), Ok(id));
            assert_eq!(id.info().id, id);
        }
        assert!("zed".parse::<AppId>().is_err());
        assert_eq!(AppId::Toolbox.info().bundle_name, "Ferrum Toolbox");
        assert_eq!(AppId::Term.info().extra_binaries, ["fterm"]);
    }
}

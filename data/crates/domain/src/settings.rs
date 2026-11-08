//! Persisted user settings and workspace state (versioned via serde defaults).

use crate::{EditorTabId, ProfileId, QueryLimits, TableRef};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Theme {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct LayoutSettings {
    pub sidebar_width: f64,
    pub results_height: f64,
}

impl Default for LayoutSettings {
    fn default() -> Self {
        Self {
            sidebar_width: 260.0,
            results_height: 320.0,
        }
    }
}

impl LayoutSettings {
    pub const MIN_SIDEBAR: f64 = 180.0;
    pub const MAX_SIDEBAR: f64 = 600.0;
    pub const MIN_RESULTS: f64 = 120.0;
    pub const MAX_RESULTS: f64 = 900.0;

    #[must_use]
    pub fn clamped(self) -> Self {
        Self {
            sidebar_width: self
                .sidebar_width
                .clamp(Self::MIN_SIDEBAR, Self::MAX_SIDEBAR),
            results_height: self
                .results_height
                .clamp(Self::MIN_RESULTS, Self::MAX_RESULTS),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    pub theme: Theme,
    pub layout: LayoutSettings,
    pub query_limits: QueryLimits,
}

/// An editor tab as restored on next launch. Contains SQL text, never secrets.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditorTabState {
    pub id: EditorTabId,
    pub title: String,
    pub sql: String,
    pub profile_id: Option<ProfileId>,
    pub position: i64,
    /// Set for table-browser tabs.
    #[serde(default)]
    pub table: Option<TableRef>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unknown_or_missing_fields_fall_back_to_defaults() {
        let s: AppSettings = serde_json::from_str(r#"{"theme":"Dark","future_field":1}"#)
            .expect("forward compatible");
        assert_eq!(s.theme, Theme::Dark);
        assert_eq!(s.layout, LayoutSettings::default());
    }

    #[test]
    fn layout_is_clamped() {
        let l = LayoutSettings {
            sidebar_width: 5.0,
            results_height: 5000.0,
        }
        .clamped();
        assert_eq!(l.sidebar_width, LayoutSettings::MIN_SIDEBAR);
        assert_eq!(l.results_height, LayoutSettings::MAX_RESULTS);
    }
}

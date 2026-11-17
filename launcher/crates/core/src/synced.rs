//! The launcher's settings that would follow the user to another machine,
//! kept apart from what only makes sense on this one, so a future sync
//! through the Ferrum account (which has no settings API yet) can carry
//! [`SyncedSettings`] as one JSON document without touching the rest.
//!
//! Synced: the release repository, the check interval, versions to keep,
//! background updates, the menu bar icon, usage counts and each app's
//! update channel. Machine-local: installed and active versions, global
//! shortcuts (they depend on what else the machine binds), the apps
//! already announced, one-time migration markers, and the shell commands
//! and Dock icons, which live in `~/.ferrum/preferences.json` shared with
//! every Ferrum app (a sync of those belongs to `ferrum-shell`).
//!
//! Nothing here talks to a server: [`State::synced_settings`] reads the
//! document and [`State::apply_synced_settings`] adopts one.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::catalog::AppId;
use crate::release::UpdateChannel;
use crate::store::State;

/// Bumped when a field changes meaning, so an older launcher can tell a
/// document it doesn't understand.
pub const SYNCED_FORMAT: u32 = 1;

/// The synced part of the launcher's settings. Unknown fields are ignored
/// and missing ones keep this machine's value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SyncedSettings {
    pub format: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_owner: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repo_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub check_interval_hours: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keep_versions: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_update: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub menu_bar: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub share_usage_counts: Option<bool>,
    /// Update channel per app.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub channels: BTreeMap<AppId, UpdateChannel>,
}

impl State {
    /// This machine's synced settings, every field filled in.
    #[must_use]
    pub fn synced_settings(&self) -> SyncedSettings {
        let settings = &self.settings;
        SyncedSettings {
            format: SYNCED_FORMAT,
            repo_owner: Some(settings.repo_owner.clone()),
            repo_name: Some(settings.repo_name.clone()),
            check_interval_hours: Some(settings.check_interval_hours),
            keep_versions: Some(settings.keep_versions),
            auto_update: Some(settings.auto_update),
            menu_bar: Some(settings.menu_bar),
            share_usage_counts: Some(settings.share_usage_counts),
            channels: self
                .apps
                .iter()
                .map(|(app, state)| (*app, state.channel))
                .collect(),
        }
    }

    /// Adopts `synced`: each field present and valid replaces this
    /// machine's value. A document of a newer format is refused whole.
    /// Returns whether anything changed (the caller saves and, for the
    /// repo or interval, re-checks).
    pub fn apply_synced_settings(&mut self, synced: &SyncedSettings) -> bool {
        if synced.format > SYNCED_FORMAT {
            tracing::warn!(
                format = synced.format,
                "synced settings from a newer launcher; ignored"
            );
            return false;
        }
        let before = (self.settings.clone(), self.apps.clone());
        let settings = &mut self.settings;
        let name = |text: &Option<String>| text.clone().filter(|t| valid_repo_part(t));
        if let (Some(owner), Some(repo)) = (name(&synced.repo_owner), name(&synced.repo_name)) {
            settings.repo_owner = owner;
            settings.repo_name = repo;
        }
        if let Some(hours) = synced
            .check_interval_hours
            .filter(|h| (1..=168).contains(h))
        {
            settings.check_interval_hours = hours;
        }
        if let Some(keep) = synced.keep_versions.filter(|k| *k <= 50) {
            settings.keep_versions = keep;
        }
        if let Some(on) = synced.auto_update {
            settings.auto_update = on;
        }
        if let Some(on) = synced.menu_bar {
            settings.menu_bar = on;
        }
        if let Some(on) = synced.share_usage_counts {
            settings.share_usage_counts = on;
        }
        for (app, channel) in &synced.channels {
            self.apps.entry(*app).or_default().channel = *channel;
        }
        (self.settings.clone(), self.apps.clone()) != before
    }
}

/// A GitHub owner or repository name: letters, digits, `-`, `_` and `.`.
fn valid_repo_part(text: &str) -> bool {
    !text.is_empty()
        && text.len() <= 100
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::store::InstalledVersion;

    fn installed(version: &str) -> InstalledVersion {
        InstalledVersion {
            version: semver::Version::parse(version).expect("version"),
            tag: format!("git-v{version}"),
            asset: "git.tar.gz".into(),
            size: 1,
            installed_at: chrono::Utc::now(),
            notes: String::new(),
        }
    }

    #[test]
    fn round_trips_between_machines_without_local_state() {
        let mut laptop = State::default();
        laptop.settings.check_interval_hours = 12;
        laptop.settings.auto_update = true;
        laptop.settings.keep_versions = 1;
        laptop.apps.entry(AppId::GIT).or_default().channel = UpdateChannel::Stable;
        laptop.apps.entry(AppId::GIT).or_default().installed = vec![installed("0.3.0")];
        laptop
            .settings
            .app_prefs
            .entry(AppId::GIT)
            .or_default()
            .hotkey = Some("cmd+alt+g".into());

        let json = serde_json::to_string(&laptop.synced_settings()).expect("serialize");
        assert!(!json.contains("installed"), "{json}");
        assert!(!json.contains("hotkey"), "{json}");
        assert!(!json.contains("seen_apps"), "{json}");

        let mut desktop = State::default();
        desktop.settings.seen_apps = Some([AppId::EDITOR].into_iter().collect());
        let synced: SyncedSettings = serde_json::from_str(&json).expect("parse");
        assert!(desktop.apply_synced_settings(&synced));
        assert_eq!(desktop.settings.check_interval_hours, 12);
        assert!(desktop.settings.auto_update);
        assert_eq!(desktop.settings.keep_versions, 1);
        assert_eq!(desktop.channel(AppId::GIT), UpdateChannel::Stable);
        // Machine-local state stays as it was.
        assert!(desktop.app(AppId::GIT).installed.is_empty());
        assert!(desktop.settings.app_prefs.is_empty());
        assert_eq!(
            desktop.settings.seen_apps.as_ref().map(|s| s.len()),
            Some(1)
        );
        // Applying the same document again changes nothing.
        assert!(!desktop.apply_synced_settings(&synced));
        assert_eq!(desktop.synced_settings(), laptop.synced_settings());
    }

    #[test]
    fn partial_invalid_and_newer_documents() {
        let mut state = State::default();
        let partial: SyncedSettings =
            serde_json::from_str(r#"{"format":1,"menu_bar":true,"future_field":3}"#)
                .expect("parse");
        assert!(state.apply_synced_settings(&partial));
        assert!(state.settings.menu_bar);
        assert_eq!(
            state.settings,
            crate::Settings {
                menu_bar: true,
                ..Default::default()
            }
        );

        let invalid: SyncedSettings = serde_json::from_str(
            r#"{"format":1,"repo_owner":"a b","repo_name":"x","check_interval_hours":0,"keep_versions":999}"#,
        )
        .expect("parse");
        assert!(!state.apply_synced_settings(&invalid));

        let newer: SyncedSettings =
            serde_json::from_str(r#"{"format":2,"menu_bar":false}"#).expect("parse");
        assert!(!state.apply_synced_settings(&newer));
        assert!(state.settings.menu_bar);
    }
}

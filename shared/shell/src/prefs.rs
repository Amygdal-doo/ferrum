//! `~/.ferrum/preferences.json`: the integration choices that Ferrum
//! Launcher and every app read and write (Settings › System in each app,
//! the app pages in the launcher):
//!
//! ```json
//! { "hide_from_dock": ["ferrumeditor"],
//!   "shell_commands": { "editor": { "name": "fei", "enabled": true } },
//!   "commands_revision": 1,
//!   "ferrum_command": true }
//! ```
//!
//! `shell_commands` is missing until the launcher (or an app) first moves
//! the commands here from the launcher's `state.json`; an app without an
//! entry has its suggested name, turned off. Fields this crate doesn't
//! know are kept.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::apps::{self, APPS};

/// The file's name in `~/.ferrum`.
pub const FILE: &str = "preferences.json";

/// Bumped by one-time moves of the suggested commands (see
/// [`RENAMED_DEFAULTS`]). 1: FerrumIDE `fee` → `fei`, FerrumEngineer `fev`
/// → `fen`, FerrumVariables `fen` → `fev`.
pub const COMMANDS_REVISION: u32 = 1;

/// Suggested commands that changed with the 2026-11 renames: (app, old,
/// new). The `fen` / `fev` pair swapped between two apps.
pub const RENAMED_DEFAULTS: [(&str, &str, &str); 3] = [
    ("editor", "fee", "fei"),
    ("vibe", "fev", "fen"),
    ("env", "fen", "fev"),
];

/// One app's shell command.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandPref {
    pub name: String,
    pub enabled: bool,
}

/// See the module docs.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SharedPrefs {
    /// Binaries (`ferrumeditor`, …) that run without a Dock icon.
    pub hide_from_dock: BTreeSet<String>,
    /// Per app id; `None` until moved here from the launcher's state.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub shell_commands: Option<BTreeMap<String, CommandPref>>,
    /// See [`COMMANDS_REVISION`].
    pub commands_revision: u32,
    /// The `ferrum <app>` command; on unless turned off.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub ferrum_command: Option<bool>,
    #[serde(flatten)]
    pub other: serde_json::Map<String, serde_json::Value>,
}

/// `<ferrum_dir>/preferences.json`.
#[must_use]
pub fn path(ferrum_dir: &Path) -> PathBuf {
    ferrum_dir.join(FILE)
}

impl SharedPrefs {
    /// Reads `path`; defaults when it is missing or unreadable.
    #[must_use]
    pub fn load(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    /// Writes atomically (temp file + rename).
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        let partial = path.with_extension(format!("json.{}.partial", std::process::id()));
        std::fs::write(&partial, json)?;
        std::fs::rename(&partial, path)
    }

    /// `app_id`'s command: its entry, or its suggested name turned off.
    #[must_use]
    pub fn command(&self, app_id: &str) -> CommandPref {
        self.shell_commands
            .as_ref()
            .and_then(|commands| commands.get(app_id).cloned())
            .unwrap_or_else(|| CommandPref {
                name: apps::by_id(app_id)
                    .map(|app| app.default_command.to_owned())
                    .unwrap_or_default(),
                enabled: false,
            })
    }

    /// `app_id`'s command name while it is on.
    #[must_use]
    pub fn enabled_command(&self, app_id: &str) -> Option<String> {
        let command = self.command(app_id);
        command.enabled.then_some(command.name)
    }

    pub fn set_command(&mut self, app_id: &str, name: &str, enabled: bool) {
        self.shell_commands.get_or_insert_default().insert(
            app_id.to_owned(),
            CommandPref {
                name: name.to_owned(),
                enabled,
            },
        );
    }

    /// Whether `ferrum <app>` is installed.
    #[must_use]
    pub fn umbrella_enabled(&self) -> bool {
        self.ferrum_command.unwrap_or(true)
    }

    pub fn set_hidden(&mut self, binary: &str, hidden: bool) {
        if hidden {
            self.hide_from_dock.insert(binary.to_owned());
        } else {
            self.hide_from_dock.remove(binary);
        }
    }

    /// Takes the commands that are on from an older source (the launcher's
    /// `state.json`, or the scripts in `~/.ferrum/bin`), at its
    /// `revision`, unless the file has its own already. Returns whether it
    /// did.
    pub fn migrate_from(
        &mut self,
        enabled: impl IntoIterator<Item = (String, String)>,
        revision: u32,
    ) -> bool {
        if self.shell_commands.is_some() {
            return false;
        }
        let commands = enabled
            .into_iter()
            .map(|(app, name)| {
                let pref = CommandPref {
                    name,
                    enabled: true,
                };
                (app, pref)
            })
            .collect();
        self.shell_commands = Some(commands);
        self.commands_revision = revision;
        true
    }

    /// Makes the one-time moves of [`RENAMED_DEFAULTS`] not made yet.
    /// Returns the moves (app, old, new).
    pub fn apply_renames(&mut self) -> Vec<(String, String, String)> {
        if self.commands_revision >= COMMANDS_REVISION {
            return Vec::new();
        }
        self.commands_revision = COMMANDS_REVISION;
        let Some(commands) = self.shell_commands.as_mut() else {
            return Vec::new();
        };
        let names = commands
            .iter()
            .map(|(app, pref)| (app.clone(), pref.name.clone()))
            .collect();
        let moved = rename_defaults(&names);
        for (app, _, new) in &moved {
            if let Some(pref) = commands.get_mut(app) {
                pref.name.clone_from(new);
            }
        }
        moved
    }

    /// Every script `~/.ferrum/bin` should hold: (name, text).
    #[must_use]
    pub fn wanted_scripts(&self, bin_dir: &Path) -> Vec<(String, String)> {
        let mut wanted: Vec<(String, String)> = APPS
            .iter()
            .filter_map(|app| {
                let name = self.enabled_command(app.id)?;
                let entry = bin_dir.join(app.binary);
                let text = crate::script::app_script(app, &name, &entry);
                Some((name, text))
            })
            .collect();
        if self.umbrella_enabled() {
            wanted.push((
                crate::umbrella::NAME.to_owned(),
                crate::umbrella::script(bin_dir, self),
            ));
        }
        wanted
    }
}

/// The moves of [`RENAMED_DEFAULTS`] for the apps in `names` (app → its
/// command) that still have the old name. A move that would give two apps
/// the same name (the new one is someone else's) is not made.
#[must_use]
pub fn rename_defaults(names: &BTreeMap<String, String>) -> Vec<(String, String, String)> {
    let mut moved: Vec<(String, String, String)> = RENAMED_DEFAULTS
        .iter()
        .filter(|(app, old, _)| names.get(*app).is_some_and(|name| name == old))
        .map(|(app, old, new)| ((*app).to_owned(), (*old).to_owned(), (*new).to_owned()))
        .collect();
    // A move whose new name is still held by another app (after the other
    // moves) is dropped, which can free or take a name in turn.
    loop {
        let name_of = |app: &str| {
            moved
                .iter()
                .find(|(a, _, _)| a == app)
                .map(|(_, _, new)| new.clone())
                .or_else(|| names.get(app).cloned())
        };
        let clash = moved.iter().position(|(app, _, new)| {
            names
                .keys()
                .any(|other| other != app && name_of(other).as_deref() == Some(new.as_str()))
        });
        match clash {
            Some(index) => {
                moved.remove(index);
            }
            None => break,
        }
    }
    moved
}

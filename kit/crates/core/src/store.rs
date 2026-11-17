//! Input history and named saved inputs per tool, kept in one JSON file
//! (`~/.ferrum/kit/inputs.json`, chosen by the app). An entry is the text
//! of the tool's input fields in a fixed order; modes and toggles are not
//! stored.
//!
//! History is off for [sensitive](Tool::sensitive) tools (JWT) unless the
//! user turns it on; turning it off deletes what was kept.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::catalog::Tool;

/// Most recent inputs kept per tool.
pub const HISTORY_LIMIT: usize = 20;

/// One remembered input: the fields' text and when it was recorded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub inputs: Vec<String>,
    /// Unix seconds.
    pub at: i64,
}

impl Entry {
    /// The first non-empty line of each non-empty field, joined with ` · `
    /// and cut to `max` characters, for the history list.
    #[must_use]
    pub fn preview(&self, max: usize) -> String {
        preview(&self.inputs, max)
    }
}

/// An input the user saved under a name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SavedInput {
    pub name: String,
    pub inputs: Vec<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
struct ToolState {
    /// Newest first.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    history: Vec<Entry>,
    /// Sorted by name.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    saved: Vec<SavedInput>,
    /// `None`: the tool's default (off for sensitive tools).
    #[serde(skip_serializing_if = "Option::is_none")]
    keep_history: Option<bool>,
}

/// Everything FerrumKit keeps between launches.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct KitStore {
    /// By [`Tool::id`].
    tools: BTreeMap<String, ToolState>,
}

/// See [`Entry::preview`].
#[must_use]
pub fn preview(inputs: &[String], max: usize) -> String {
    let text = inputs
        .iter()
        .filter_map(|input| input.lines().map(str::trim).find(|l| !l.is_empty()))
        .collect::<Vec<_>>()
        .join(" · ");
    if text.chars().count() > max {
        let cut: String = text.chars().take(max.saturating_sub(1)).collect();
        format!("{cut}…")
    } else {
        text
    }
}

fn is_blank(inputs: &[String]) -> bool {
    inputs.iter().all(|input| input.trim().is_empty())
}

impl KitStore {
    /// Reads the store; a missing file is an empty store, a corrupt one an
    /// error.
    pub fn load(path: &Path) -> Result<Self, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} is not valid: {e}", path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(format!("Could not read {}: {e}", path.display())),
        }
    }

    /// Writes the store (through a temporary file, readable only by the
    /// user since inputs may hold secrets).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let fail = |e: std::io::Error| format!("Could not write {}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(fail)?;
        }
        let text = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(fail)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600)).map_err(fail)?;
        }
        std::fs::rename(&tmp, path).map_err(fail)
    }

    fn state(&self, tool: Tool) -> Option<&ToolState> {
        self.tools.get(tool.id())
    }

    fn state_mut(&mut self, tool: Tool) -> &mut ToolState {
        self.tools.entry(tool.id().to_string()).or_default()
    }

    /// Whether `tool`'s inputs are recorded.
    #[must_use]
    pub fn keeps_history(&self, tool: Tool) -> bool {
        self.state(tool)
            .and_then(|s| s.keep_history)
            .unwrap_or(!tool.sensitive())
    }

    /// Turns history on or off for `tool`; off also deletes it.
    pub fn set_keeps_history(&mut self, tool: Tool, keep: bool) {
        let state = self.state_mut(tool);
        state.keep_history = (keep == tool.sensitive()).then_some(keep);
        if !keep {
            state.history.clear();
        }
    }

    /// Newest first.
    #[must_use]
    pub fn history(&self, tool: Tool) -> &[Entry] {
        self.state(tool).map_or(&[], |s| s.history.as_slice())
    }

    /// Records `inputs` as the newest entry. With `replace_latest` (the same
    /// editing session) it overwrites the newest entry instead of adding
    /// one. An input already in the list moves to the top. Blank inputs and
    /// tools without history are skipped. Returns whether anything changed.
    pub fn record(&mut self, tool: Tool, inputs: &[String], at: i64, replace_latest: bool) -> bool {
        if !self.keeps_history(tool) || is_blank(inputs) {
            return false;
        }
        let history = &mut self.state_mut(tool).history;
        if history.first().is_some_and(|e| e.inputs == inputs) {
            return false;
        }
        if replace_latest && !history.is_empty() {
            history.remove(0);
        }
        history.retain(|e| e.inputs != inputs);
        history.insert(
            0,
            Entry {
                inputs: inputs.to_vec(),
                at,
            },
        );
        history.truncate(HISTORY_LIMIT);
        true
    }

    /// Removes the history entry at `index` (newest is 0).
    pub fn remove_history(&mut self, tool: Tool, index: usize) -> bool {
        let history = &mut self.state_mut(tool).history;
        if index < history.len() {
            history.remove(index);
            true
        } else {
            false
        }
    }

    pub fn clear_history(&mut self, tool: Tool) {
        self.state_mut(tool).history.clear();
    }

    /// Sorted by name.
    #[must_use]
    pub fn saved(&self, tool: Tool) -> &[SavedInput] {
        self.state(tool).map_or(&[], |s| s.saved.as_slice())
    }

    /// Saves `inputs` as `name`, replacing an input saved under that name.
    pub fn save_named(&mut self, tool: Tool, name: &str, inputs: &[String]) -> Result<(), String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Give the input a name".to_string());
        }
        if is_blank(inputs) {
            return Err("Nothing to save: the input is empty".to_string());
        }
        let saved = &mut self.state_mut(tool).saved;
        saved.retain(|s| s.name != name);
        saved.push(SavedInput {
            name: name.to_string(),
            inputs: inputs.to_vec(),
        });
        saved.sort_by_key(|s| s.name.to_lowercase());
        Ok(())
    }

    pub fn remove_saved(&mut self, tool: Tool, name: &str) -> bool {
        let saved = &mut self.state_mut(tool).saved;
        let before = saved.len();
        saved.retain(|s| s.name != name);
        saved.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inputs(text: &str) -> Vec<String> {
        vec![text.to_string()]
    }

    #[test]
    fn records_newest_first_with_a_limit() {
        let mut store = KitStore::default();
        for i in 0..(HISTORY_LIMIT + 5) {
            assert!(store.record(Tool::Base64, &inputs(&format!("v{i}")), i as i64, false));
        }
        let history = store.history(Tool::Base64);
        assert_eq!(history.len(), HISTORY_LIMIT);
        assert_eq!(
            history[0].inputs,
            inputs(&format!("v{}", HISTORY_LIMIT + 4))
        );
        assert!(store.history(Tool::Json).is_empty());
    }

    #[test]
    fn dedupes_replaces_and_skips_blank() {
        let mut store = KitStore::default();
        assert!(!store.record(Tool::Json, &inputs("  \n"), 1, false));
        assert!(store.record(Tool::Json, &inputs("a"), 1, false));
        assert!(store.record(Tool::Json, &inputs("b"), 2, false));
        // The same input again moves to the top instead of repeating.
        assert!(store.record(Tool::Json, &inputs("a"), 3, false));
        let texts: Vec<_> = store
            .history(Tool::Json)
            .iter()
            .map(|e| e.inputs[0].clone())
            .collect();
        assert_eq!(texts, ["a", "b"]);
        assert!(!store.record(Tool::Json, &inputs("a"), 4, false));
        // Same editing session: the newest entry is overwritten.
        assert!(store.record(Tool::Json, &inputs("ab"), 5, true));
        assert_eq!(store.history(Tool::Json).len(), 2);
        assert_eq!(store.history(Tool::Json)[0].inputs, inputs("ab"));
        assert!(store.remove_history(Tool::Json, 1));
        assert!(!store.remove_history(Tool::Json, 5));
        store.clear_history(Tool::Json);
        assert!(store.history(Tool::Json).is_empty());
    }

    #[test]
    fn sensitive_tools_keep_no_history_by_default() {
        let mut store = KitStore::default();
        assert!(!store.keeps_history(Tool::Jwt));
        assert!(!store.record(Tool::Jwt, &inputs("eyJ"), 1, false));
        store.set_keeps_history(Tool::Jwt, true);
        assert!(store.record(Tool::Jwt, &inputs("eyJ"), 1, false));
        // Turning it off deletes what was kept.
        store.set_keeps_history(Tool::Jwt, false);
        assert!(store.history(Tool::Jwt).is_empty());
        store.set_keeps_history(Tool::Hash, false);
        assert!(!store.record(Tool::Hash, &inputs("x"), 1, false));
    }

    #[test]
    fn saved_inputs_by_name() {
        let mut store = KitStore::default();
        assert!(store.save_named(Tool::Regex, " ", &inputs("x")).is_err());
        assert!(store.save_named(Tool::Regex, "e", &inputs("")).is_err());
        store
            .save_named(Tool::Regex, "email", &inputs("a"))
            .unwrap();
        store.save_named(Tool::Regex, "Date", &inputs("b")).unwrap();
        store
            .save_named(Tool::Regex, "email", &inputs("c"))
            .unwrap();
        let names: Vec<_> = store.saved(Tool::Regex).iter().map(|s| &s.name).collect();
        assert_eq!(names, ["Date", "email"]);
        assert_eq!(store.saved(Tool::Regex)[1].inputs, inputs("c"));
        assert!(store.remove_saved(Tool::Regex, "Date"));
        assert!(!store.remove_saved(Tool::Regex, "Date"));
    }

    #[test]
    fn previews_fields() {
        let entry = Entry {
            inputs: vec!["\n  {\"a\": 1}\n".into(), String::new(), "gi".into()],
            at: 0,
        };
        assert_eq!(entry.preview(80), "{\"a\": 1} · gi");
        assert_eq!(entry.preview(5), "{\"a\"…");
    }

    #[test]
    fn round_trips_through_a_file() {
        let dir = std::env::temp_dir().join(format!("ferrumkit-store-{}", std::process::id()));
        let path = dir.join("kit").join("inputs.json");
        assert_eq!(KitStore::load(&path).unwrap(), KitStore::default());
        let mut store = KitStore::default();
        store.record(Tool::Url, &inputs("https://x"), 7, false);
        store.save_named(Tool::Url, "x", &inputs("y")).unwrap();
        store.set_keeps_history(Tool::Jwt, true);
        store.save(&path).unwrap();
        assert_eq!(KitStore::load(&path).unwrap(), store);
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).unwrap().permissions().mode();
            assert_eq!(mode & 0o777, 0o600);
        }
        std::fs::write(&path, "{").unwrap();
        assert!(KitStore::load(&path).is_err());
        std::fs::remove_dir_all(&dir).ok();
    }
}

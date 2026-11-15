//! Sidebar folders: named groups of terminals (tabs, or sessions when the
//! tabs run across the top), e.g. one per project. A folder can be
//! collapsed, renamed, deleted (its terminals go back to the top level and
//! keep running) and given a directory that "New terminal in this folder"
//! starts in.
//!
//! Kept in `<state dir>/sidebar-folders.json`, so per machine. Terminals are
//! referred to by their server ids (`w3`, `s2`), which a restarted server
//! reuses: the file remembers which server run its memberships belong to
//! and forgets them (not the folders) when that changes.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Folder {
    pub id: u64,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
    /// Directory for "New terminal in this folder".
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Terminal ids (`w3`, `s2`) in the order they were added.
    #[serde(default)]
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Folders {
    pub folders: Vec<Folder>,
    next: u64,
    /// `started_ms` of the server whose ids `items` refer to.
    server: Option<u64>,
}

/// The sidebar's order: each folder with its terminals, then the rest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layout<'a> {
    pub folders: Vec<(&'a Folder, Vec<String>)>,
    pub loose: Vec<String>,
}

fn file() -> PathBuf {
    ferrumterm_client::paths::state_dir().join("sidebar-folders.json")
}

impl Folders {
    #[must_use]
    pub fn load() -> Self {
        std::fs::read_to_string(file())
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let path = file();
        let written = serde_json::to_string_pretty(self)
            .map_err(std::io::Error::other)
            .and_then(|text| {
                path.parent()
                    .map_or(Ok(()), std::fs::create_dir_all)
                    .and_then(|()| std::fs::write(&path, text))
            });
        if let Err(error) = written {
            tracing::warn!(%error, path = %path.display(), "could not save sidebar folders");
        }
    }

    /// Adopts the ids of the server started at `started_ms`; true when the
    /// memberships of another server run were dropped.
    pub fn use_server(&mut self, started_ms: u64) -> bool {
        if self.server == Some(started_ms) {
            return false;
        }
        let had_items = self.folders.iter().any(|f| !f.items.is_empty());
        self.server = Some(started_ms);
        for f in &mut self.folders {
            f.items.clear();
        }
        had_items
    }

    /// A new folder, named `name` (`Folder`, `Folder 2`… when empty or taken).
    pub fn create(&mut self, name: &str) -> u64 {
        let base = if name.trim().is_empty() {
            "Folder"
        } else {
            name.trim()
        };
        let taken = |n: &str| self.folders.iter().any(|f| f.name == n);
        let name = std::iter::once(base.to_owned())
            .chain((2..).map(|i| format!("{base} {i}")))
            .find(|n| !taken(n))
            .unwrap_or_else(|| base.to_owned());
        self.next += 1;
        let id = self.next;
        self.folders.push(Folder {
            id,
            name,
            collapsed: false,
            path: None,
            items: Vec::new(),
        });
        id
    }

    #[must_use]
    pub fn get(&self, id: u64) -> Option<&Folder> {
        self.folders.iter().find(|f| f.id == id)
    }

    fn get_mut(&mut self, id: u64) -> Option<&mut Folder> {
        self.folders.iter_mut().find(|f| f.id == id)
    }

    pub fn rename(&mut self, id: u64, name: &str) -> bool {
        let name = name.trim();
        match self.get_mut(id) {
            Some(f) if !name.is_empty() => {
                f.name = name.to_owned();
                true
            }
            _ => false,
        }
    }

    /// Deletes a folder; its terminals return to the top level.
    pub fn delete(&mut self, id: u64) -> bool {
        let before = self.folders.len();
        self.folders.retain(|f| f.id != id);
        before != self.folders.len()
    }

    pub fn toggle(&mut self, id: u64) {
        if let Some(f) = self.get_mut(id) {
            f.collapsed = !f.collapsed;
        }
    }

    /// Sets (or with an empty `path`, clears) the folder's directory.
    pub fn set_path(&mut self, id: u64, path: &str) {
        if let Some(f) = self.get_mut(id) {
            let path = path.trim();
            f.path = (!path.is_empty()).then(|| path.to_owned());
        }
    }

    /// The folder holding `item`.
    #[must_use]
    pub fn folder_of(&self, item: &str) -> Option<u64> {
        self.folders
            .iter()
            .find(|f| f.items.iter().any(|i| i == item))
            .map(|f| f.id)
    }

    /// Moves `item` into folder `to`, or (`None`) back to the top level.
    pub fn move_item(&mut self, item: &str, to: Option<u64>) {
        if to.is_some_and(|id| self.get(id).is_none()) {
            return;
        }
        for f in &mut self.folders {
            f.items.retain(|i| i != item);
        }
        if let Some(f) = to.and_then(|id| self.get_mut(id)) {
            f.items.push(item.to_owned());
        }
    }

    /// Forgets terminals that no longer exist.
    pub fn prune(&mut self, alive: impl Fn(&str) -> bool) -> bool {
        let mut changed = false;
        for f in &mut self.folders {
            let before = f.items.len();
            f.items.retain(|i| alive(i));
            changed |= before != f.items.len();
        }
        changed
    }

    /// Groups `items` (the sidebar's terminals, in order) by folder; a
    /// folder lists its terminals in `items` order.
    #[must_use]
    pub fn layout(&self, items: &[String]) -> Layout<'_> {
        let folders = self
            .folders
            .iter()
            .map(|f| {
                let inside = items
                    .iter()
                    .filter(|i| f.items.contains(i))
                    .cloned()
                    .collect();
                (f, inside)
            })
            .collect();
        let loose = items
            .iter()
            .filter(|i| self.folder_of(i).is_none())
            .cloned()
            .collect();
        Layout { folders, loose }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ids(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn create_rename_toggle_and_delete() {
        let mut f = Folders::default();
        let a = f.create("");
        let b = f.create("Folder");
        assert_eq!(f.get(a).map(|x| x.name.as_str()), Some("Folder"));
        assert_eq!(f.get(b).map(|x| x.name.as_str()), Some("Folder 2"));
        assert!(f.rename(b, " ferrum "));
        assert!(!f.rename(b, "  "), "names can't be empty");
        assert_eq!(f.get(b).map(|x| x.name.as_str()), Some("ferrum"));
        f.toggle(a);
        assert!(f.get(a).is_some_and(|x| x.collapsed));
        f.set_path(b, "~/src/ferrum");
        assert_eq!(
            f.get(b).and_then(|x| x.path.as_deref()),
            Some("~/src/ferrum")
        );
        f.set_path(b, "");
        assert_eq!(f.get(b).and_then(|x| x.path.clone()), None);
        f.move_item("w1", Some(a));
        assert!(f.delete(a));
        assert!(!f.delete(a));
        assert_eq!(f.folder_of("w1"), None, "back at the top level");
        // Ids are not reused after a delete.
        assert_ne!(f.create("x"), a);
    }

    #[test]
    fn moving_terminals_and_layout() {
        let mut f = Folders::default();
        let web = f.create("web");
        let api = f.create("api");
        f.move_item("w3", Some(web));
        f.move_item("w1", Some(web));
        f.move_item("w2", Some(api));
        f.move_item("w2", Some(web));
        f.move_item("w9", Some(99));
        assert_eq!(f.folder_of("w9"), None, "unknown folder: no move");
        let items = ids(&["w1", "w2", "w3", "w4"]);
        let layout = f.layout(&items);
        assert_eq!(layout.folders[0].0.name, "web");
        assert_eq!(layout.folders[0].1, ids(&["w1", "w2", "w3"]));
        assert!(layout.folders[1].1.is_empty());
        assert_eq!(layout.loose, ids(&["w4"]));
        f.move_item("w1", None);
        assert_eq!(f.layout(&items).loose, ids(&["w1", "w4"]));
    }

    #[test]
    fn prunes_and_forgets_another_server_run() {
        let mut f = Folders::default();
        let a = f.create("a");
        assert!(!f.use_server(1), "nothing to forget yet");
        f.move_item("w1", Some(a));
        f.move_item("w2", Some(a));
        assert!(f.prune(|i| i == "w2"));
        assert_eq!(f.get(a).map(|x| x.items.clone()), Some(ids(&["w2"])));
        assert!(!f.use_server(1));
        assert!(f.use_server(2), "a restarted server reuses ids");
        assert!(f.get(a).is_some_and(|x| x.items.is_empty()));
        let json = serde_json::to_string(&f).expect("json");
        assert_eq!(serde_json::from_str::<Folders>(&json).expect("parse"), f);
    }
}

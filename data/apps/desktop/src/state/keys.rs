//! Key explorer and key tab state for key-value (Redis) connections.

use ferrumdata_domain::{KeyEntry, KeyScanPage, KeyValueView};
use std::collections::HashSet;

/// Keys loaded so far for one connection: `SCAN` pages appended in order.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct KeyBrowser {
    /// Glob the loaded keys were scanned with.
    pub pattern: String,
    pub keys: Vec<KeyEntry>,
    names: HashSet<String>,
    /// `SCAN` cursor for the next page.
    pub cursor: u64,
    /// The scan reached the end of the keyspace.
    pub complete: bool,
    pub loading: bool,
    pub error: Option<String>,
    /// `DBSIZE` when the scan started.
    pub total: Option<u64>,
    /// Expanded folder prefixes (`user:`, `user:1:`).
    pub expanded: HashSet<String>,
    /// Bumped on every new scan so late pages of an older scan are dropped.
    pub generation: u64,
}

impl KeyBrowser {
    /// Starts over with `pattern`; folders stay expanded.
    pub fn restart(&mut self, pattern: String) {
        self.pattern = pattern;
        self.keys.clear();
        self.names.clear();
        self.cursor = 0;
        self.complete = false;
        self.loading = true;
        self.error = None;
        self.total = None;
        self.generation += 1;
    }

    /// Appends a page. `SCAN` may repeat keys across pages: they are skipped.
    pub fn append(&mut self, page: KeyScanPage) {
        self.complete = page.is_complete();
        for key in page.keys {
            if self.names.insert(key.name.clone()) {
                self.keys.push(key);
            }
        }
        self.cursor = page.cursor;
        self.loading = false;
        self.error = None;
    }

    /// Drops a deleted key from the list.
    pub fn remove(&mut self, name: &str) {
        if self.names.remove(name) {
            self.keys.retain(|k| k.name != name);
        }
    }

    /// Refreshes a listed key's type and TTL from a fresh value view.
    pub fn update_from(&mut self, view: &KeyValueView) {
        if let Some(k) = self.keys.iter_mut().find(|k| k.name == view.key) {
            k.kind = view.kind.clone();
            k.ttl = view.ttl;
        }
    }
}

/// The value shown in a key tab.
#[derive(Debug, Clone, PartialEq)]
pub enum KeyViewLoad {
    Loading,
    Loaded(Box<KeyValueView>),
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{KeyKind, KeyTtl};

    fn key(name: &str) -> KeyEntry {
        KeyEntry {
            name: name.into(),
            kind: KeyKind::String,
            ttl: KeyTtl::Persistent,
            binary: false,
        }
    }

    #[test]
    fn pages_append_without_duplicates_and_restart_clears() {
        let mut b = KeyBrowser::default();
        b.restart("user:*".into());
        assert!(b.loading);
        b.append(KeyScanPage {
            keys: vec![key("a"), key("b")],
            cursor: 7,
        });
        b.append(KeyScanPage {
            keys: vec![key("b"), key("c")],
            cursor: 0,
        });
        let names: Vec<_> = b.keys.iter().map(|k| k.name.as_str()).collect();
        assert_eq!(names, ["a", "b", "c"]);
        assert!(b.complete);
        b.remove("b");
        assert_eq!(b.keys.len(), 2);
        let generation = b.generation;
        b.restart("*".into());
        assert!(b.keys.is_empty());
        assert_eq!(b.generation, generation + 1);
        // A key removed earlier can be listed again after a restart.
        b.append(KeyScanPage {
            keys: vec![key("b")],
            cursor: 0,
        });
        assert_eq!(b.keys.len(), 1);
    }
}

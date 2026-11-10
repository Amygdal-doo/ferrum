//! Groups loaded key names into a `:`-separated tree for the explorer
//! (`user:1:name` → `user` / `1` / `name`). Only loaded keys are grouped, so
//! folder counts cover the keys scanned so far, not the whole database.

use ferrumdata_domain::KeyEntry;
use std::collections::HashSet;

pub const SEPARATOR: char = ':';

/// One visible row of the key tree, in display order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KeyTreeRow {
    Folder {
        /// Full prefix including the trailing separator, e.g. `user:1:`.
        prefix: String,
        /// Last segment, e.g. `1`.
        label: String,
        depth: usize,
        /// Loaded keys under this folder.
        keys: usize,
        expanded: bool,
    },
    Key {
        /// Index into the `keys` slice passed to [`key_tree_rows`].
        index: usize,
        /// Name after the parent folder's prefix.
        label: String,
        depth: usize,
    },
}

/// Visible rows for `keys`, at most `limit`; the flag is true when rows were
/// cut. `expand_all` opens every folder (used while filtering).
#[must_use]
pub fn key_tree_rows(
    keys: &[KeyEntry],
    expanded: &HashSet<String>,
    expand_all: bool,
    limit: usize,
) -> (Vec<KeyTreeRow>, bool) {
    let mut order: Vec<usize> = (0..keys.len()).collect();
    order.sort_by(|&a, &b| keys[a].name.cmp(&keys[b].name));
    let mut walk = Walk {
        keys,
        expanded,
        expand_all,
        limit,
        out: Vec::new(),
        cut: false,
    };
    walk.level(&order, 0, 0);
    (walk.out, walk.cut)
}

struct Walk<'a> {
    keys: &'a [KeyEntry],
    expanded: &'a HashSet<String>,
    expand_all: bool,
    limit: usize,
    out: Vec<KeyTreeRow>,
    cut: bool,
}

impl Walk<'_> {
    fn push(&mut self, row: KeyTreeRow) -> bool {
        if self.out.len() >= self.limit {
            self.cut = true;
            return false;
        }
        self.out.push(row);
        true
    }

    /// `order` is sorted by name and every name in it starts with the same
    /// `prefix_len` bytes, so each folder's keys form one contiguous run.
    fn level(&mut self, order: &[usize], prefix_len: usize, depth: usize) {
        let mut i = 0;
        while i < order.len() {
            let name = &self.keys[order[i]].name;
            let rest = &name[prefix_len..];
            let Some(pos) = rest.find(SEPARATOR) else {
                let row = KeyTreeRow::Key {
                    index: order[i],
                    label: rest.to_owned(),
                    depth,
                };
                if !self.push(row) {
                    return;
                }
                i += 1;
                continue;
            };
            let end = prefix_len + pos + SEPARATOR.len_utf8();
            let prefix = &name[..end];
            let run = order[i..]
                .iter()
                .take_while(|&&k| self.keys[k].name.starts_with(prefix))
                .count();
            let open = self.expand_all || self.expanded.contains(prefix);
            let row = KeyTreeRow::Folder {
                prefix: prefix.to_owned(),
                label: rest[..pos].to_owned(),
                depth,
                keys: run,
                expanded: open,
            };
            if !self.push(row) {
                return;
            }
            if open {
                self.level(&order[i..i + run], end, depth + 1);
                if self.cut {
                    return;
                }
            }
            i += run;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{KeyKind, KeyTtl};

    fn keys(names: &[&str]) -> Vec<KeyEntry> {
        names
            .iter()
            .map(|n| KeyEntry {
                name: (*n).to_owned(),
                kind: KeyKind::String,
                ttl: KeyTtl::Persistent,
                binary: false,
            })
            .collect()
    }

    fn render(rows: &[KeyTreeRow]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                KeyTreeRow::Folder {
                    label,
                    depth,
                    keys,
                    expanded,
                    ..
                } => format!(
                    "{}{}{label} ({keys})",
                    "  ".repeat(*depth),
                    if *expanded { "v " } else { "> " }
                ),
                KeyTreeRow::Key { label, depth, .. } => format!("{}{label}", "  ".repeat(*depth)),
            })
            .collect()
    }

    #[test]
    fn collapsed_folders_show_counts_in_name_order() {
        let k = keys(&["user:2", "session:a", "user:1:name", "plain", "user:1:age"]);
        let (rows, cut) = key_tree_rows(&k, &HashSet::new(), false, 100);
        assert!(!cut);
        assert_eq!(render(&rows), ["plain", "> session (1)", "> user (3)"]);
    }

    #[test]
    fn expanded_folders_nest_and_keep_leaf_keys_next_to_folders() {
        let k = keys(&["user:2", "user:1:name", "user:1:age", "user", "user:"]);
        let expanded: HashSet<String> = ["user:".to_owned(), "user:1:".to_owned()].into();
        let (rows, _) = key_tree_rows(&k, &expanded, false, 100);
        assert_eq!(
            render(&rows),
            [
                "user",
                "v user (4)",
                "  ",
                "  v 1 (2)",
                "    age",
                "    name",
                "  2"
            ]
        );
        // Key rows point back into the input slice.
        let KeyTreeRow::Key { index, .. } = &rows[4] else {
            panic!("{rows:?}")
        };
        assert_eq!(k[*index].name, "user:1:age");
    }

    #[test]
    fn expand_all_and_limit() {
        let k = keys(&["a:b:c", "a:d", "e"]);
        let (rows, cut) = key_tree_rows(&k, &HashSet::new(), true, 100);
        assert_eq!(render(&rows), ["v a (2)", "  v b (1)", "    c", "  d", "e"]);
        assert!(!cut);
        let (rows, cut) = key_tree_rows(&k, &HashSet::new(), true, 3);
        assert_eq!(rows.len(), 3);
        assert!(cut);
    }

    #[test]
    fn prefixes_sharing_a_start_are_separate_folders() {
        let k = keys(&["ab:1", "a:1", "a-b:1"]);
        let (rows, _) = key_tree_rows(&k, &HashSet::new(), false, 100);
        assert_eq!(render(&rows), ["> a-b (1)", "> a (1)", "> ab (1)"]);
    }
}

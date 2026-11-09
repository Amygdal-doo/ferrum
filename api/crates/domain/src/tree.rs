//! Tree operations on [`Collection`].

use crate::auth::Auth;
use crate::collection::{Collection, Item, SavedRequest};
use crate::ids::{CollectionId, ItemId};

fn find_in(items: &[Item], id: ItemId) -> Option<&Item> {
    items.iter().find_map(|it| {
        if it.id() == id {
            Some(it)
        } else if let Item::Folder(f) = it {
            find_in(&f.items, id)
        } else {
            None
        }
    })
}

fn find_in_mut(items: &mut [Item], id: ItemId) -> Option<&mut Item> {
    for it in items.iter_mut() {
        if it.id() == id {
            return Some(it);
        }
        if let Item::Folder(f) = it
            && let Some(found) = find_in_mut(&mut f.items, id)
        {
            return Some(found);
        }
    }
    None
}

/// Finds the vector containing `id` and its index there.
fn parent_vec_mut(items: &mut Vec<Item>, id: ItemId) -> Option<(&mut Vec<Item>, usize)> {
    if let Some(i) = items.iter().position(|it| it.id() == id) {
        return Some((items, i));
    }
    for it in items.iter_mut() {
        if let Item::Folder(f) = it
            && let Some(found) = parent_vec_mut(&mut f.items, id)
        {
            return Some(found);
        }
    }
    None
}

/// Auth chain from the root down to `id` (inclusive). Empty if not found.
fn auth_path<'a>(items: &'a [Item], id: ItemId, acc: &mut Vec<&'a Auth>) -> bool {
    for it in items {
        match it {
            Item::Request(r) if r.id == id => {
                acc.push(&r.request.auth);
                return true;
            }
            Item::Folder(f) => {
                acc.push(&f.auth);
                if f.id == id || auth_path(&f.items, id, acc) {
                    return true;
                }
                acc.pop();
            }
            Item::Request(_) => {}
        }
    }
    false
}

fn walk_requests<'a>(items: &'a [Item], out: &mut Vec<&'a SavedRequest>) {
    for it in items {
        match it {
            Item::Request(r) => out.push(r),
            Item::Folder(f) => walk_requests(&f.items, out),
        }
    }
}

impl Collection {
    #[must_use]
    pub fn find(&self, id: ItemId) -> Option<&Item> {
        find_in(&self.items, id)
    }

    pub fn find_mut(&mut self, id: ItemId) -> Option<&mut Item> {
        find_in_mut(&mut self.items, id)
    }

    #[must_use]
    pub fn find_request(&self, id: ItemId) -> Option<&SavedRequest> {
        match self.find(id)? {
            Item::Request(r) => Some(r),
            Item::Folder(_) => None,
        }
    }

    pub fn find_request_mut(&mut self, id: ItemId) -> Option<&mut SavedRequest> {
        match self.find_mut(id)? {
            Item::Request(r) => Some(r),
            Item::Folder(_) => None,
        }
    }

    /// Appends `item` to the folder `parent` (or the root when `None`).
    /// Returns `false` if `parent` is not a folder in this collection.
    pub fn insert(&mut self, parent: Option<ItemId>, item: Item) -> bool {
        match parent {
            None => {
                self.items.push(item);
                true
            }
            Some(pid) => match self.find_mut(pid) {
                Some(Item::Folder(f)) => {
                    f.items.push(item);
                    true
                }
                _ => false,
            },
        }
    }

    pub fn remove(&mut self, id: ItemId) -> Option<Item> {
        let (vec, i) = parent_vec_mut(&mut self.items, id)?;
        Some(vec.remove(i))
    }

    pub fn rename(&mut self, id: ItemId, name: impl Into<String>) -> bool {
        self.find_mut(id).map(|it| it.set_name(name)).is_some()
    }

    /// Duplicates an item (deep, fresh ids) right after the original and
    /// returns the copy's id.
    pub fn duplicate_item(&mut self, id: ItemId) -> Option<ItemId> {
        let (vec, i) = parent_vec_mut(&mut self.items, id)?;
        let mut copy = vec[i].with_fresh_ids();
        let name = format!("{} Copy", copy.name());
        copy.set_name(name);
        let new_id = copy.id();
        vec.insert(i + 1, copy);
        Some(new_id)
    }

    /// Whole-collection copy with fresh ids.
    #[must_use]
    pub fn duplicate(&self) -> Self {
        Self {
            id: CollectionId::new(),
            name: format!("{} Copy", self.name),
            items: self.items.iter().map(Item::with_fresh_ids).collect(),
            ..self.clone()
        }
    }

    /// Resolves `Inherit` for the item `id` by walking up through folders to
    /// the collection. Never returns `Inherit`: an unresolved chain is `None`.
    #[must_use]
    pub fn effective_auth(&self, id: ItemId) -> Auth {
        let mut chain = vec![&self.auth];
        if !auth_path(&self.items, id, &mut chain) {
            return Auth::None;
        }
        chain
            .into_iter()
            .rev()
            .find(|a| !a.is_inherit())
            .cloned()
            .unwrap_or(Auth::None)
    }

    /// Every saved request, depth-first.
    #[must_use]
    pub fn requests(&self) -> Vec<&SavedRequest> {
        let mut out = Vec::new();
        walk_requests(&self.items, &mut out);
        out
    }

    /// Saved requests inside `folder` (the whole collection when `None`),
    /// depth-first in tree order. Empty if `folder` is not a folder here.
    #[must_use]
    pub fn requests_under(&self, folder: Option<ItemId>) -> Vec<&SavedRequest> {
        match folder {
            None => self.requests(),
            Some(id) => match self.find(id) {
                Some(Item::Folder(f)) => {
                    let mut out = Vec::new();
                    walk_requests(&f.items, &mut out);
                    out
                }
                _ => Vec::new(),
            },
        }
    }
}

#[cfg(test)]
#[path = "tree_tests.rs"]
mod tests;

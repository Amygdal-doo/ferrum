//! Collection tree mutations. Each applies to the in-memory list first, then
//! persists the whole collection in the background.

use super::run;
use super::workspace::open_request;
use crate::state::{AppState, Dialog, tabs};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::{
    Collection, CollectionId, Folder, HttpRequest, Item, ItemId, RequestOrigin, SavedRequest, TabId,
};

/// Saves `c` in the background; errors become a notice.
pub fn persist(state: AppState, services: AppServices, c: Collection) {
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(
            &services,
            async move { svc.store.save_collection(&c).await },
        )
        .await
        {
            state.notify(format!("Could not save collection: {e}"), true);
        }
    });
}

/// Applies `f` to collection `id`, persists it and fixes tab origins.
pub fn mutate<R>(
    mut state: AppState,
    services: AppServices,
    id: CollectionId,
    f: impl FnOnce(&mut Collection) -> R,
) -> Option<R> {
    let (result, snapshot) = {
        let mut list = state.collections.write();
        let c = list.iter_mut().find(|c| c.id == id)?;
        let r = f(c);
        (r, c.clone())
    };
    persist(state, services, snapshot);
    reconcile(state);
    Some(result)
}

fn reconcile(mut state: AppState) {
    let collections = state.collections.peek().clone();
    if tabs::reconcile_origins(&mut state.tabs.write(), &collections) {
        state.touch();
    }
}

pub fn create(mut state: AppState, services: AppServices, name: &str) -> CollectionId {
    let c = Collection::new(name);
    let id = c.id;
    state.collections.write().push(c.clone());
    state.expanded.write().insert(id.0);
    persist(state, services, c);
    id
}

/// Adds a new folder and starts renaming it.
pub fn add_folder(
    mut state: AppState,
    services: AppServices,
    c: CollectionId,
    parent: Option<ItemId>,
) {
    let folder = Folder::new("New Folder");
    let fid = folder.id;
    if mutate(state, services, c, |col| {
        col.insert(parent, Item::Folder(folder))
    }) == Some(true)
    {
        expand(state, c, parent);
        state.renaming.set(Some(fid.0));
    }
}

/// Adds a new request and opens it in a tab.
pub fn add_request(
    state: AppState,
    services: AppServices,
    c: CollectionId,
    parent: Option<ItemId>,
) {
    let saved = SavedRequest::new("New Request", HttpRequest::default());
    let item = saved.id;
    let request = saved.request.clone();
    if mutate(state, services, c, |col| {
        col.insert(parent, Item::Request(saved))
    }) == Some(true)
    {
        expand(state, c, parent);
        let origin = RequestOrigin {
            collection: c,
            item,
        };
        open_request(state, "New Request".into(), request, Some(origin));
    }
}

fn expand(mut state: AppState, c: CollectionId, parent: Option<ItemId>) {
    let mut set = state.expanded.write();
    set.insert(c.0);
    if let Some(p) = parent {
        set.insert(p.0);
    }
}

/// Renames a collection (`item: None`) or an item, and retitles open tabs.
pub fn rename(
    mut state: AppState,
    services: AppServices,
    c: CollectionId,
    item: Option<ItemId>,
    name: String,
) {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return;
    }
    match item {
        None => {
            let n = name.clone();
            mutate(state, services, c, move |col| col.name = n);
        }
        Some(id) => {
            let n = name.clone();
            mutate(state, services, c, move |col| col.rename(id, n));
            let origin = RequestOrigin {
                collection: c,
                item: id,
            };
            for tab in state
                .tabs
                .write()
                .iter_mut()
                .filter(|t| t.origin == Some(origin))
            {
                tab.title.clone_from(&name);
            }
            state.touch();
        }
    }
}

pub fn duplicate(
    mut state: AppState,
    services: AppServices,
    c: CollectionId,
    item: Option<ItemId>,
) {
    match item {
        Some(id) => {
            mutate(state, services, c, |col| col.duplicate_item(id));
        }
        None => {
            let Some(copy) = state.collection(c).map(|col| col.duplicate()) else {
                return;
            };
            state.collections.write().push(copy.clone());
            persist(state, services, copy);
        }
    }
}

pub fn delete(mut state: AppState, services: AppServices, c: CollectionId, item: Option<ItemId>) {
    match item {
        Some(id) => {
            mutate(state, services, c, |col| col.remove(id));
        }
        None => {
            state.collections.write().retain(|col| col.id != c);
            reconcile(state);
            let svc = services.clone();
            spawn(async move {
                if let Err(e) = run(
                    &services,
                    async move { svc.store.delete_collection(c).await },
                )
                .await
                {
                    state.notify(format!("Could not delete collection: {e}"), true);
                }
            });
        }
    }
}

/// Replaces a collection's metadata (name, auth, variables) from the settings dialog.
pub fn update_meta(state: AppState, services: AppServices, edited: Collection) {
    mutate(state, services, edited.id, move |col| {
        col.name = edited.name;
        col.auth = edited.auth;
        col.variables = edited.variables;
    });
}

/// Cmd/Ctrl+S: writes the tab back to its origin, or asks where to save it.
pub fn save_tab(mut state: AppState, services: AppServices, tab_id: TabId) {
    let Some(tab) = state.tabs.peek().iter().find(|t| t.id == tab_id).cloned() else {
        return;
    };
    let target = tab.origin.filter(|o| {
        state
            .collection(o.collection)
            .is_some_and(|c| c.find_request(o.item).is_some())
    });
    let Some(o) = target else {
        state.dialog.set(Some(Dialog::SaveRequest(tab_id)));
        return;
    };
    let request = tab.request.clone();
    mutate(state, services, o.collection, move |col| {
        if let Some(saved) = col.find_request_mut(o.item) {
            saved.request = request;
        }
    });
    state.update_tab(tab_id, |t| t.dirty = false);
    state.notify("Saved.", false);
}

/// Saves the tab as a new request in `c` under `parent`.
pub fn save_tab_as(
    state: AppState,
    services: AppServices,
    tab_id: TabId,
    c: CollectionId,
    parent: Option<ItemId>,
    name: String,
) {
    let Some(tab) = state.tabs.peek().iter().find(|t| t.id == tab_id).cloned() else {
        return;
    };
    let saved = SavedRequest::new(name.clone(), tab.request.clone());
    let item = saved.id;
    if mutate(state, services, c, |col| {
        col.insert(parent, Item::Request(saved))
    }) == Some(true)
    {
        expand(state, c, parent);
        state.update_tab(tab_id, |t| {
            t.origin = Some(RequestOrigin {
                collection: c,
                item,
            });
            t.title = name;
            t.dirty = false;
        });
    }
}

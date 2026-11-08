//! Collection tree mutations. Each applies to the in-memory list first, then
//! persists the whole collection in the background.

use crate::app::FerrumApi;
use crate::state::{Dialog, SettingsPane, tabs};
use ferrumapi_domain::{
    Collection, CollectionId, Folder, HttpRequest, Item, ItemId, RequestOrigin, SavedRequest, TabId,
};
use gpui::Context;

impl FerrumApi {
    /// Saves `c` in the background; errors become a notice.
    pub fn persist_collection(&mut self, c: Collection, cx: &mut Context<Self>) {
        let svc = self.services.clone();
        self.io(
            cx,
            async move { svc.store.save_collection(&c).await },
            Self::report("Could not save collection"),
        );
    }

    /// Applies `f` to collection `id`, persists it and fixes tab origins.
    pub fn mutate_collection<R>(
        &mut self,
        id: CollectionId,
        cx: &mut Context<Self>,
        f: impl FnOnce(&mut Collection) -> R,
    ) -> Option<R> {
        let c = self.state.collections.iter_mut().find(|c| c.id == id)?;
        let result = f(c);
        let snapshot = c.clone();
        self.persist_collection(snapshot, cx);
        self.reconcile_origins();
        Some(result)
    }

    fn reconcile_origins(&mut self) {
        let state = &mut self.state;
        if tabs::reconcile_origins(&mut state.tabs, &state.collections) {
            state.touch();
        }
    }

    pub fn create_collection(&mut self, name: &str, cx: &mut Context<Self>) -> CollectionId {
        let c = Collection::new(name);
        let id = c.id;
        self.state.collections.push(c.clone());
        self.state.expanded.insert(id.0);
        self.persist_collection(c, cx);
        id
    }

    /// Adds a new folder and starts renaming it.
    pub fn add_folder(&mut self, c: CollectionId, parent: Option<ItemId>, cx: &mut Context<Self>) {
        let folder = Folder::new("New Folder");
        let fid = folder.id;
        if self.mutate_collection(c, cx, |col| col.insert(parent, Item::Folder(folder)))
            == Some(true)
        {
            self.expand(c, parent);
            self.state.renaming = Some(fid.0);
        }
    }

    /// Adds a new request and opens it in a tab.
    pub fn add_request(&mut self, c: CollectionId, parent: Option<ItemId>, cx: &mut Context<Self>) {
        let saved = SavedRequest::new("New Request", HttpRequest::default());
        let item = saved.id;
        let request = saved.request.clone();
        if self.mutate_collection(c, cx, |col| col.insert(parent, Item::Request(saved)))
            == Some(true)
        {
            self.expand(c, parent);
            let origin = RequestOrigin {
                collection: c,
                item,
            };
            self.open_request("New Request".into(), request, Some(origin));
        }
    }

    fn expand(&mut self, c: CollectionId, parent: Option<ItemId>) {
        self.state.expanded.insert(c.0);
        if let Some(p) = parent {
            self.state.expanded.insert(p.0);
        }
    }

    /// Renames a collection (`item: None`) or an item, and retitles open tabs.
    pub fn rename_item(
        &mut self,
        c: CollectionId,
        item: Option<ItemId>,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let name = name.trim().to_owned();
        if name.is_empty() {
            return;
        }
        match item {
            None => {
                self.mutate_collection(c, cx, move |col| col.name = name);
            }
            Some(id) => {
                let n = name.clone();
                self.mutate_collection(c, cx, move |col| col.rename(id, n));
                let origin = RequestOrigin {
                    collection: c,
                    item: id,
                };
                for tab in self
                    .state
                    .tabs
                    .iter_mut()
                    .filter(|t| t.origin == Some(origin))
                {
                    tab.title.clone_from(&name);
                }
                self.state.touch();
            }
        }
    }

    pub fn duplicate_item(
        &mut self,
        c: CollectionId,
        item: Option<ItemId>,
        cx: &mut Context<Self>,
    ) {
        match item {
            Some(id) => {
                self.mutate_collection(c, cx, |col| col.duplicate_item(id));
            }
            None => {
                let Some(copy) = self.state.collection(c).map(Collection::duplicate) else {
                    return;
                };
                self.state.collections.push(copy.clone());
                self.persist_collection(copy, cx);
            }
        }
    }

    pub fn delete_item(&mut self, c: CollectionId, item: Option<ItemId>, cx: &mut Context<Self>) {
        match item {
            Some(id) => {
                self.mutate_collection(c, cx, |col| col.remove(id));
            }
            None => {
                self.state.collections.retain(|col| col.id != c);
                self.reconcile_origins();
                let svc = self.services.clone();
                self.io(
                    cx,
                    async move { svc.store.delete_collection(c).await },
                    Self::report("Could not delete collection"),
                );
            }
        }
    }

    pub fn open_collection_settings(&mut self, c: CollectionId) {
        if let Some(col) = self.state.collection(c) {
            self.state.dialog = Some(Dialog::CollectionSettings {
                draft: Box::new(col.clone()),
                pane: SettingsPane::Auth,
            });
        }
    }

    /// Replaces a collection's metadata (name, auth, variables) from the settings dialog.
    pub fn update_collection_meta(&mut self, edited: Collection, cx: &mut Context<Self>) {
        self.mutate_collection(edited.id, cx, move |col| {
            col.name = edited.name;
            col.auth = edited.auth;
            col.variables = edited.variables;
        });
    }

    /// Cmd/Ctrl+S: writes the tab back to its origin, or asks where to save it.
    pub fn save_tab(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        let Some(tab) = self.state.tab(tab_id).cloned() else {
            return;
        };
        let target = tab.origin.filter(|o| {
            self.state
                .collection(o.collection)
                .is_some_and(|c| c.find_request(o.item).is_some())
        });
        let Some(o) = target else {
            self.open_save_dialog(tab_id);
            return;
        };
        let request = tab.request.clone();
        self.mutate_collection(o.collection, cx, move |col| {
            if let Some(saved) = col.find_request_mut(o.item) {
                saved.request = request;
            }
        });
        self.state.update_tab(tab_id, |t| t.dirty = false);
        self.notify("Saved.", false, cx);
    }

    pub fn open_save_dialog(&mut self, tab_id: TabId) {
        let name = self
            .state
            .tab(tab_id)
            .map(tabs::display_title)
            .unwrap_or_default();
        let target = self.state.collections.first().map(|c| (c.id, None));
        self.state.dialog = Some(Dialog::SaveRequest {
            tab_id,
            name,
            new_collection: "New Collection".to_owned(),
            target,
        });
    }

    /// Saves the tab as a new request in `c` under `parent`.
    pub fn save_tab_as(
        &mut self,
        tab_id: TabId,
        c: CollectionId,
        parent: Option<ItemId>,
        name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.state.tab(tab_id).cloned() else {
            return;
        };
        let saved = SavedRequest::new(name.clone(), tab.request.clone());
        let item = saved.id;
        if self.mutate_collection(c, cx, |col| col.insert(parent, Item::Request(saved)))
            == Some(true)
        {
            self.expand(c, parent);
            self.state.update_tab(tab_id, |t| {
                t.origin = Some(RequestOrigin {
                    collection: c,
                    item,
                });
                t.title = name;
                t.dirty = false;
            });
        }
    }
}

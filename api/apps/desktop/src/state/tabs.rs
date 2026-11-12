//! Pure tab-list reducers (unit tested; no signals).

use ferrumapi_domain::{Collection, HttpRequest, RequestOrigin, RequestTab, TabId, WorkspaceState};

/// Label shown on a tab: the custom title, or the request label.
#[must_use]
pub fn display_title(tab: &RequestTab) -> String {
    if tab.title.trim().is_empty() {
        tab.request.label()
    } else {
        tab.title.clone()
    }
}

/// A fresh, empty request tab.
#[must_use]
pub fn blank_tab() -> RequestTab {
    RequestTab::new(String::new(), HttpRequest::default())
}

/// Removes `id` and returns the tab that should become active. Always leaves
/// at least one tab.
pub fn close(tabs: &mut Vec<RequestTab>, active: Option<TabId>, id: TabId) -> Option<TabId> {
    let Some(idx) = tabs.iter().position(|t| t.id == id) else {
        return active;
    };
    tabs.remove(idx);
    if tabs.is_empty() {
        tabs.push(blank_tab());
    }
    if active == Some(id) || active.is_none() {
        tabs.get(idx.saturating_sub(1))
            .or(tabs.first())
            .map(|t| t.id)
    } else {
        active
    }
}

/// The tab already showing `origin`, if any.
#[must_use]
pub fn find_by_origin(tabs: &[RequestTab], origin: RequestOrigin) -> Option<TabId> {
    tabs.iter().find(|t| t.origin == Some(origin)).map(|t| t.id)
}

/// Restores a saved workspace: guarantees one tab and a valid active id.
#[must_use]
pub fn restore(ws: WorkspaceState) -> (Vec<RequestTab>, Option<TabId>) {
    let mut tabs = ws.tabs;
    if tabs.is_empty() {
        tabs.push(blank_tab());
    }
    let active = ws
        .active_tab
        .filter(|id| tabs.iter().any(|t| t.id == *id))
        .or_else(|| tabs.first().map(|t| t.id));
    (tabs, active)
}

/// Drops origins that no longer point at a saved request (deleted item or
/// collection). Returns `true` if any tab changed.
pub fn reconcile_origins(tabs: &mut [RequestTab], collections: &[Collection]) -> bool {
    let mut changed = false;
    for tab in tabs.iter_mut() {
        let Some(o) = tab.origin else { continue };
        let exists = collections
            .iter()
            .find(|c| c.id == o.collection)
            .is_some_and(|c| c.find_request(o.item).is_some());
        if !exists {
            tab.origin = None;
            tab.dirty = true;
            changed = true;
        }
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumapi_domain::{CollectionId, Item, ItemId, SavedRequest};

    fn three() -> Vec<RequestTab> {
        vec![blank_tab(), blank_tab(), blank_tab()]
    }

    #[test]
    fn closing_active_selects_left_neighbour() {
        let mut tabs = three();
        let ids: Vec<_> = tabs.iter().map(|t| t.id).collect();
        let next = close(&mut tabs, Some(ids[1]), ids[1]);
        assert_eq!(next, Some(ids[0]));
        assert_eq!(tabs.len(), 2);
    }

    #[test]
    fn closing_inactive_keeps_active() {
        let mut tabs = three();
        let ids: Vec<_> = tabs.iter().map(|t| t.id).collect();
        assert_eq!(close(&mut tabs, Some(ids[2]), ids[0]), Some(ids[2]));
    }

    #[test]
    fn closing_last_tab_leaves_a_blank_one() {
        let mut tabs = vec![blank_tab()];
        let id = tabs[0].id;
        let next = close(&mut tabs, Some(id), id);
        assert_eq!(tabs.len(), 1);
        assert_ne!(tabs[0].id, id);
        assert_eq!(next, Some(tabs[0].id));
    }

    #[test]
    fn restore_fixes_missing_active() {
        let (tabs, active) = restore(WorkspaceState::default());
        assert_eq!(tabs.len(), 1);
        assert_eq!(active, Some(tabs[0].id));
        let ws = WorkspaceState {
            tabs: three(),
            active_tab: Some(TabId::new()),
            active_environment: None,
            project: None,
        };
        let first = ws.tabs[0].id;
        assert_eq!(restore(ws).1, Some(first));
    }

    #[test]
    fn reconcile_clears_dangling_origins() {
        let mut c = Collection::new("c");
        let saved = SavedRequest::new("r", HttpRequest::default());
        let kept = RequestOrigin {
            collection: c.id,
            item: saved.id,
        };
        c.insert(None, Item::Request(saved));
        let mut tabs = three();
        tabs[0].origin = Some(kept);
        tabs[1].origin = Some(RequestOrigin {
            collection: CollectionId::new(),
            item: ItemId::new(),
        });
        assert!(reconcile_origins(&mut tabs, &[c]));
        assert_eq!(tabs[0].origin, Some(kept));
        assert_eq!(tabs[1].origin, None);
        assert_eq!(find_by_origin(&tabs, kept), Some(tabs[0].id));
    }

    #[test]
    fn display_title_falls_back_to_label() {
        let mut t = blank_tab();
        assert_eq!(display_title(&t), "Untitled request");
        t.request.url = "https://api.dev/users".into();
        assert_eq!(display_title(&t), "api.dev/users");
        t.title = "Users".into();
        assert_eq!(display_title(&t), "Users");
    }
}

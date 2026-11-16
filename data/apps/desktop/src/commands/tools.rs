//! Tool tabs: opening them and loading the schema snapshot they work on.

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{DiagramView, EditorTab, SnapshotLoad, ToolTab};
use ferrum_ui::prelude::*;
use ferrumdata_application::er;
use ferrumdata_domain::{EditorTabId, ProfileId};
use std::sync::Arc;

impl FerrumData {
    /// Focuses the connection's tool tab of this kind, or opens one.
    fn open_tool(&mut self, profile_id: ProfileId, tool: ToolTab, title: String) -> EditorTabId {
        let existing = self
            .state
            .tabs
            .iter()
            .find(|t| t.profile_id == Some(profile_id) && t.tool == Some(tool))
            .map(|t| t.id);
        if let Some(id) = existing {
            self.state.active_tab = Some(id);
            return id;
        }
        let mut tab = EditorTab::new(0, Some(profile_id));
        tab.title = title;
        tab.tool = Some(tool);
        let id = tab.id;
        self.state.tabs.push(tab);
        self.state.active_tab = Some(id);
        id
    }

    pub fn open_diagram(&mut self, profile_id: ProfileId, cx: &mut Context<Self>) {
        let name = self
            .state
            .profile(profile_id)
            .map_or_else(String::new, |p| p.display_name.clone());
        let tab_id = self.open_tool(profile_id, ToolTab::Diagram, format!("ER · {name}"));
        if !self.state.diagrams.contains_key(&tab_id) {
            self.load_diagram(tab_id, profile_id, cx);
        }
        cx.notify();
    }

    /// (Re)loads the schema and lays it out off the UI thread.
    pub fn load_diagram(
        &mut self,
        tab_id: EditorTabId,
        profile_id: ProfileId,
        cx: &mut Context<Self>,
    ) {
        let view = self.state.diagrams.entry(tab_id).or_default();
        view.load = SnapshotLoad::Loading;
        view.fit_pending = true;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                let snapshot = svc.connections.describe_schema(profile_id).await?;
                let layout = er::layout(&snapshot);
                Ok::<_, ferrumdata_domain::DbError>((snapshot, layout))
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let Some(view) = this.state.diagrams.get_mut(&tab_id) else {
                    return;
                };
                match result {
                    Ok(Ok((snapshot, layout))) => {
                        view.load = SnapshotLoad::Loaded(Arc::new(snapshot));
                        view.layout = Some(Arc::new(layout));
                    }
                    Ok(Err(e)) => view.load = SnapshotLoad::Failed(e.message),
                    Err(e) => view.load = SnapshotLoad::Failed(e),
                }
                cx.notify();
            }));
        })
        .detach();
    }

    /// Drops a closed tab's tool state.
    pub(crate) fn forget_tool(&mut self, tab_id: EditorTabId) {
        self.state.diagrams.remove(&tab_id);
    }

    #[must_use]
    pub fn diagram(&self, tab_id: EditorTabId) -> Option<&DiagramView> {
        self.state.diagrams.get(&tab_id)
    }
}

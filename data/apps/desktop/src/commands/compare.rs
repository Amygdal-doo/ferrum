//! Schema compare tab: loads both schemas, diffs them and generates the
//! migration script. The script is shown (and can be opened in a SQL tab)
//! but never run from here.

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{CompareLoad, CompareView, EditorTab, ToolTab};
use ferrum_ui::prelude::*;
use ferrumdata_application::compare::{diff, migration};
use ferrumdata_domain::{EditorTabId, ProfileId};

impl FerrumData {
    pub fn open_compare(&mut self, profile_id: ProfileId, cx: &mut Context<Self>) {
        let name = self
            .state
            .profile(profile_id)
            .map_or_else(String::new, |p| p.display_name.clone());
        let tab_id = self.open_tool(profile_id, ToolTab::Compare, format!("Compare · {name}"));
        self.state.compares.entry(tab_id).or_default();
        cx.notify();
    }

    /// Connections the source can be compared with: same engine, not itself.
    pub fn compare_targets(&self, source: ProfileId) -> Vec<(ProfileId, String)> {
        let Some(engine) = self.state.profile(source).map(|p| p.engine) else {
            return Vec::new();
        };
        self.state
            .profiles
            .iter()
            .filter(|p| p.engine == engine && p.id != source)
            .map(|p| (p.id, p.display_name.clone()))
            .collect()
    }

    pub fn run_compare(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(source) = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.profile_id)
        else {
            return;
        };
        let Some(target) = self.state.compares.get(&tab_id).and_then(|v| v.target) else {
            return;
        };
        let Some(engine) = self.state.profile(source).map(|p| p.engine) else {
            return;
        };
        if let Some(view) = self.state.compares.get_mut(&tab_id) {
            view.load = CompareLoad::Loading;
        }
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                let (s, t) = tokio::join!(
                    svc.connections.describe_schema(source),
                    svc.connections.describe_schema(target)
                );
                let (s, t) = (s?, t?);
                let changes = diff(engine, &s, &t);
                let script = migration(engine, &changes, &s, &t);
                Ok::<_, ferrumdata_domain::DbError>((changes, script))
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let load = match result {
                    Ok(Ok((diff, script))) => CompareLoad::Loaded { diff, script },
                    Ok(Err(e)) => CompareLoad::Failed(e.message),
                    Err(e) => CompareLoad::Failed(e),
                };
                if let Some(view) = this.state.compares.get_mut(&tab_id) {
                    // Ignore a result for a target changed meanwhile.
                    if view.target == Some(target) {
                        view.load = load;
                    }
                }
                cx.notify();
            }));
        })
        .detach();
    }

    pub fn set_compare_target(
        &mut self,
        tab_id: EditorTabId,
        target: ProfileId,
        cx: &mut Context<Self>,
    ) {
        self.state.compares.insert(
            tab_id,
            CompareView {
                target: Some(target),
                load: CompareLoad::Idle,
            },
        );
        self.run_compare(tab_id, cx);
    }

    /// Opens the migration in a new SQL tab on the target, for review.
    pub fn open_migration(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(view) = self.state.compares.get(&tab_id) else {
            return;
        };
        let (Some(target), CompareLoad::Loaded { script, .. }) = (view.target, &view.load) else {
            return;
        };
        let mut tab = EditorTab::new(self.state.tabs.len() + 1, Some(target));
        tab.title = "Migration".into();
        tab.sql = script.clone();
        tab.unsaved = true;
        self.state.active_tab = Some(tab.id);
        self.state.tabs.push(tab);
        self.save_tabs(cx);
        cx.notify();
    }
}

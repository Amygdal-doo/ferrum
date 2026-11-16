//! Loading and saving settings and editor tabs.

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::EditorTab;
use ferrum_ui::prelude::*;
use ferrumdata_domain::EditorTabId;
use std::time::Duration;

const AUTOSAVE_INTERVAL: Duration = Duration::from_millis(1500);

impl FerrumData {
    /// Loads settings, profiles and tabs. Never fails hard: on error the app
    /// starts empty and shows a notice.
    pub fn bootstrap(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let loaded = background(&services, async move {
                let settings = svc.store.load_settings().await;
                let profiles = svc.connections.list_profiles().await;
                let tabs = svc.store.load_tabs().await;
                (settings, profiles, tabs)
            })
            .await;

            log_dropped(this.update(cx, |this, cx| {
                match loaded {
                    Ok((settings, profiles, tabs)) => {
                        match settings {
                            Ok(s) => this.state.settings = s,
                            Err(e) => {
                                this.notify_user(format!("Could not load settings: {e}"), true, cx)
                            }
                        }
                        match profiles {
                            Ok(p) => this.state.profiles = p,
                            Err(e) => this.notify_user(
                                format!("Could not load connections: {e}"),
                                true,
                                cx,
                            ),
                        }
                        let mut tabs: Vec<EditorTab> = tabs
                            .unwrap_or_default()
                            .into_iter()
                            .map(Into::into)
                            .collect();
                        // Drop references to profiles that no longer exist.
                        let known: Vec<_> = this.state.profiles.iter().map(|p| p.id).collect();
                        for tab in &mut tabs {
                            tab.profile_id = tab.profile_id.filter(|id| known.contains(id));
                        }
                        if tabs.is_empty() {
                            tabs.push(EditorTab::new(1, known.first().copied()));
                        }
                        this.state.active_tab = tabs.first().map(|t| t.id);
                        this.state.tabs = tabs;
                    }
                    Err(e) => this.notify_user(e, true, cx),
                }
                this.state.loaded = true;
                cx.notify();
            }));
        })
        .detach();
    }

    /// Periodically persists edited tabs. Restored on next launch.
    pub fn autosave_loop(&mut self, cx: &mut Context<Self>) -> Task<()> {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(AUTOSAVE_INTERVAL).await;
                let alive = this.update(cx, |this, cx| {
                    if this.state.tabs.iter().any(|t| t.unsaved) {
                        this.save_tabs(cx);
                    }
                });
                if alive.is_err() {
                    break;
                }
            }
        })
    }

    pub fn save_tabs(&mut self, cx: &mut Context<Self>) {
        let snapshot: Vec<_> = self
            .state
            .tabs
            .iter()
            // Key and tool tabs show live state and are not restored.
            .filter(|t| t.key.is_none() && t.tool.is_none())
            .enumerate()
            .map(|(i, t)| t.to_state(i))
            .collect();
        let saved_ids: Vec<_> = snapshot.iter().map(|t| (t.id, t.sql.clone())).collect();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(
                &services,
                async move { svc.store.save_tabs(&snapshot).await },
            )
            .await;
            match result {
                Ok(Ok(())) => log_dropped(this.update(cx, |this, _| {
                    for tab in &mut this.state.tabs {
                        // Only clear the flag if nothing changed since the snapshot.
                        if saved_ids
                            .iter()
                            .any(|(id, sql)| *id == tab.id && *sql == tab.sql)
                        {
                            tab.unsaved = false;
                        }
                    }
                })),
                Ok(Err(e)) => tracing::warn!(error = %e, "failed to save tabs"),
                Err(e) => tracing::warn!(error = %e, "failed to save tabs"),
            }
        })
        .detach();
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        let settings = self.state.settings.clone();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            if let Ok(Err(e)) = background(&services, async move {
                svc.store.save_settings(&settings).await
            })
            .await
            {
                log_dropped(this.update(cx, |this, cx| {
                    this.notify_user(format!("Could not save settings: {e}"), true, cx)
                }));
            }
        })
        .detach();
    }

    pub fn new_tab(&mut self, cx: &mut Context<Self>) {
        let profile = self.state.active_tab().and_then(|t| t.profile_id);
        let n = self.state.tabs.len() + 1;
        let tab = EditorTab::new(n, profile);
        self.state.active_tab = Some(tab.id);
        self.state.tabs.push(tab);
        self.save_tabs(cx);
        cx.notify();
    }

    pub fn close_tab(&mut self, id: EditorTabId, cx: &mut Context<Self>) {
        if let Some(run) = self.state.runs.remove(&id) {
            self.services.queries.cancel(run.execution_id);
        }
        self.editors.remove(&id);
        self.table_inputs.remove(&id);
        self.key_inputs.remove(&id);
        self.state.key_views.remove(&id);
        self.forget_tool(id);
        let tabs = &mut self.state.tabs;
        let Some(idx) = tabs.iter().position(|t| t.id == id) else {
            return;
        };
        tabs.remove(idx);
        if tabs.is_empty() {
            tabs.push(EditorTab::new(1, None));
        }
        let next = tabs
            .get(idx.saturating_sub(1))
            .or(tabs.first())
            .map(|t| t.id);
        if self.state.active_tab == Some(id) {
            self.state.active_tab = next;
        }
        self.save_tabs(cx);
        cx.notify();
    }
}

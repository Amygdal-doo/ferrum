//! Table explorer: lazy table lists and opening tables in browser tabs.

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{ConnStatus, DetailsLoad, EditorTab, SchemaLoad};
use ferrum_ui::prelude::*;
use ferrumdata_application::browse::browse_query;
use ferrumdata_domain::{ProfileId, TableInfo, TableRef};

impl FerrumData {
    /// Expands/collapses a connection's table tree, loading it on first expand.
    pub fn toggle_tables(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        let now_open = if self.state.expanded.remove(&id) {
            false
        } else {
            self.state.expanded.insert(id);
            true
        };
        let key_value = self
            .state
            .profile(id)
            .is_some_and(|p| p.engine.is_key_value());
        if now_open && key_value {
            // Key-value engines list keys (SCAN pages) instead of tables.
            if !self.state.key_browsers.contains_key(&id) {
                self.scan_keys(id, true, cx);
            }
        } else if now_open && !matches!(self.state.schemas.get(&id), Some(SchemaLoad::Loaded(_))) {
            self.load_tables(id, cx);
        }
        cx.notify();
    }

    pub fn load_tables(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        self.state.schemas.insert(id, SchemaLoad::Loading);
        // Reloading the tree also refreshes column details on next expand.
        self.state.table_details.retain(|(p, _), _| *p != id);
        self.state.expanded_tables.retain(|(p, _)| *p != id);
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(
                &services,
                async move { svc.connections.list_tables(id).await },
            )
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let load = match result {
                    Ok(Ok(tables)) => {
                        if !matches!(this.state.conn_status.get(&id), Some(ConnStatus::Ready(_))) {
                            let label = this
                                .state
                                .profile(id)
                                .map_or("Connected", |p| p.engine.label());
                            this.state
                                .conn_status
                                .insert(id, ConnStatus::Ready(label.into()));
                        }
                        SchemaLoad::Loaded(tables)
                    }
                    Ok(Err(e)) => SchemaLoad::Failed(e.message),
                    Err(e) => SchemaLoad::Failed(e),
                };
                this.state.schemas.insert(id, load);
                cx.notify();
            }));
        })
        .detach();
    }

    /// Shows/hides a table's columns and indexes, loading them on first expand.
    pub fn toggle_table_details(
        &mut self,
        profile_id: ProfileId,
        table: TableRef,
        cx: &mut Context<Self>,
    ) {
        let key = (profile_id, table);
        if self.state.expanded_tables.remove(&key) {
            cx.notify();
            return;
        }
        self.state.expanded_tables.insert(key.clone());
        if !matches!(
            self.state.table_details.get(&key),
            Some(DetailsLoad::Loaded(_))
        ) {
            self.load_table_details(key.0, key.1, cx);
        }
        cx.notify();
    }

    pub fn load_table_details(
        &mut self,
        profile_id: ProfileId,
        table: TableRef,
        cx: &mut Context<Self>,
    ) {
        let key = (profile_id, table.clone());
        self.state
            .table_details
            .insert(key.clone(), DetailsLoad::Loading);
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.describe_table(profile_id, table).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let load = match result {
                    Ok(Ok(details)) => DetailsLoad::Loaded(details),
                    Ok(Err(e)) => DetailsLoad::Failed(e.message),
                    Err(e) => DetailsLoad::Failed(e),
                };
                // Dropped if the tree was reloaded meanwhile.
                if let Some(slot) = this.state.table_details.get_mut(&key) {
                    *slot = load;
                    cx.notify();
                }
            }));
        })
        .detach();
    }

    /// Focuses the tab already showing this table, or opens a new one and runs it.
    pub fn open_table(&mut self, profile_id: ProfileId, info: TableInfo, cx: &mut Context<Self>) {
        let Some(profile) = self.state.profile(profile_id).cloned() else {
            return;
        };
        let existing = self
            .state
            .tabs
            .iter()
            .find(|t| t.profile_id == Some(profile_id) && t.table.as_ref() == Some(&info.table))
            .map(|t| t.id);
        if let Some(id) = existing {
            self.state.active_tab = Some(id);
            cx.notify();
            return;
        }

        let max_rows = self.state.settings.query_limits.max_rows;
        let query = browse_query(profile.engine, &info, max_rows);
        let mut tab = EditorTab::new(0, Some(profile_id));
        tab.title = info.table.name.clone();
        tab.sql = query.sql.clone();
        tab.table = Some(info.table.clone());
        tab.unsaved = true;
        let tab_id = tab.id;
        self.state.tabs.push(tab);
        self.state.active_tab = Some(tab_id);

        self.start_run(tab_id, profile, query.sql, cx);
        if !query.deterministic
            && let Some(run) = self.state.runs.get_mut(&tab_id)
        {
            run.note =
                Some("No primary key: row order is not guaranteed between refreshes.".into());
        }
    }

    /// Re-runs a table tab's current SQL (refresh button).
    pub fn refresh_table(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.state.active_tab().cloned() else {
            return;
        };
        let Some(profile) = tab
            .profile_id
            .and_then(|id| self.state.profile(id))
            .cloned()
        else {
            return;
        };
        let note = self.state.runs.get(&tab.id).and_then(|r| r.note.clone());
        self.start_run(tab.id, profile, tab.sql.clone(), cx);
        if let Some(run) = self.state.runs.get_mut(&tab.id) {
            run.note = note;
        }
    }
}

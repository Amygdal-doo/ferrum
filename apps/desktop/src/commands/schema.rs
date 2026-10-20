//! Table explorer: lazy table lists and opening tables in browser tabs.

use super::{background, query};
use crate::state::{AppState, ConnStatus, EditorTab, SchemaLoad};
use dioxus::prelude::*;
use ferrum_application::{AppServices, browse::browse_query};
use ferrum_domain::{ProfileId, TableInfo};

/// Expands/collapses a connection's table tree, loading it on first expand.
pub fn toggle(mut state: AppState, services: AppServices, id: ProfileId) {
    let now_open = {
        let mut expanded = state.expanded.write();
        if !expanded.remove(&id) {
            expanded.insert(id);
            true
        } else {
            false
        }
    };
    if now_open && !matches!(state.schemas.peek().get(&id), Some(SchemaLoad::Loaded(_))) {
        load_tables(state, services, id);
    }
}

pub fn load_tables(mut state: AppState, services: AppServices, id: ProfileId) {
    state.schemas.write().insert(id, SchemaLoad::Loading);
    spawn(async move {
        let svc = services.clone();
        let result = background(
            &services,
            async move { svc.connections.list_tables(id).await },
        )
        .await;
        let load = match result {
            Ok(Ok(tables)) => {
                if !matches!(
                    state.conn_status.peek().get(&id),
                    Some(ConnStatus::Ready(_))
                ) {
                    state
                        .conn_status
                        .write()
                        .insert(id, ConnStatus::Ready("PostgreSQL".into()));
                }
                SchemaLoad::Loaded(tables)
            }
            Ok(Err(e)) => SchemaLoad::Failed(e.message),
            Err(e) => SchemaLoad::Failed(e),
        };
        state.schemas.write().insert(id, load);
    });
}

/// Focuses the tab already showing this table, or opens a new one and runs it.
pub fn open_table(
    mut state: AppState,
    services: AppServices,
    profile_id: ProfileId,
    info: TableInfo,
) {
    let Some(profile) = state.profile(profile_id) else {
        return;
    };
    let existing = state
        .tabs
        .peek()
        .iter()
        .find(|t| t.profile_id == Some(profile_id) && t.table.as_ref() == Some(&info.table))
        .map(|t| t.id);
    if let Some(id) = existing {
        state.active_tab.set(Some(id));
        return;
    }

    let max_rows = state.settings.peek().query_limits.max_rows;
    let query = browse_query(profile.engine, &info, max_rows);
    let mut tab = EditorTab::new(0, Some(profile_id));
    tab.title = info.table.name.clone();
    tab.sql = query.sql.clone();
    tab.table = Some(info.table.clone());
    tab.unsaved = true;
    let tab_id = tab.id;
    state.tabs.write().push(tab);
    state.active_tab.set(Some(tab_id));

    query::start_run(state, services, tab_id, profile, query.sql);
    if !query.deterministic
        && let Some(run) = state.runs.write().get_mut(&tab_id)
    {
        run.note = Some("No primary key: row order is not guaranteed between refreshes.".into());
    }
}

/// Re-runs a table tab's current SQL (refresh button).
pub fn refresh(state: AppState, services: AppServices) {
    let Some(tab) = state.active_tab() else {
        return;
    };
    let Some(profile) = tab.profile_id.and_then(|id| state.profile(id)) else {
        return;
    };
    let note = state.runs.peek().get(&tab.id).and_then(|r| r.note.clone());
    query::start_run(state, services, tab.id, profile, tab.sql.clone());
    let mut runs = state.runs;
    if let Some(run) = runs.write().get_mut(&tab.id) {
        run.note = note;
    }
}

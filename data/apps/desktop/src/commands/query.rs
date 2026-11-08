//! Running and cancelling queries from the editor.

use super::background;
use crate::state::{AppState, ConnStatus, Dialog, TabRun};
use dioxus::prelude::*;
use ferrum_application::{AppServices, safety};
use ferrum_domain::{ConnectionProfile, EditorTabId, ExecutionId, QueryRequest};
use ferrum_editor_core::{statement_at, utf16_to_byte_offset};

/// DOM id of the active editor textarea (only one is mounted at a time).
pub const EDITOR_ID: &str = "sql-editor";

/// Runs the selection, or the statement under the caret, of the active tab.
pub async fn run_from_editor(mut state: AppState, services: AppServices) {
    let Some(tab) = state.active_tab() else {
        return;
    };
    let Some(profile) = tab.profile_id.and_then(|id| state.profile(id)) else {
        state.notify("Choose a connection for this tab first.", true);
        return;
    };
    let (start, end) = editor_selection().await.unwrap_or((0, 0));
    let doc = &tab.sql;
    let sql = if start != end {
        let a = utf16_to_byte_offset(doc, start.min(end));
        let b = utf16_to_byte_offset(doc, start.max(end));
        doc[a..b].trim().trim_end_matches(';').trim().to_owned()
    } else {
        let cursor = utf16_to_byte_offset(doc, start);
        statement_at(doc, cursor)
            .map(|s| s.text(doc).to_owned())
            .unwrap_or_default()
    };
    if sql.is_empty() {
        state.notify("Nothing to run.", false);
        return;
    }
    if safety::requires_confirmation(&profile, &sql) {
        state.dialog.set(Some(Dialog::ConfirmRun {
            tab_id: tab.id,
            profile,
            sql,
        }));
        return;
    }
    start_run(state, services, tab.id, profile, sql);
}

/// Selection as UTF-16 offsets, read through the webview's textarea.
async fn editor_selection() -> Option<(usize, usize)> {
    let js = format!(
        "const el = document.getElementById('{EDITOR_ID}'); \
         return el ? [el.selectionStart, el.selectionEnd] : [0, 0];"
    );
    let v = document::eval(&js).join::<(usize, usize)>().await.ok()?;
    Some(v)
}

/// Starts a run, superseding (and cancelling) any previous run in the tab.
pub fn start_run(
    mut state: AppState,
    services: AppServices,
    tab_id: EditorTabId,
    profile: ConnectionProfile,
    sql: String,
) {
    let execution_id = ExecutionId::new();
    let limits = state.settings.peek().query_limits;
    let run = TabRun::new(execution_id, profile.id, sql.clone(), limits.max_rows);
    if let Some(previous) = state.runs.write().insert(tab_id, run) {
        services.queries.cancel(previous.execution_id);
    }
    tracing::info!(execution = %execution_id, profile = %profile.id, "query started");

    spawn(async move {
        let request = QueryRequest {
            execution_id,
            profile_id: profile.id,
            editor_id: Some(tab_id),
            sql,
            limits,
        };
        let queries = services.queries.clone();
        let started = background(&services, async move { queries.execute(request).await }).await;
        let mut rx = match started {
            Ok(Ok(rx)) => rx,
            Ok(Err(e)) => {
                state
                    .conn_status
                    .write()
                    .insert(profile.id, ConnStatus::Failed(e.message.clone()));
                with_run(state, tab_id, execution_id, |r| r.fail(e));
                return;
            }
            Err(e) => {
                with_run(state, tab_id, execution_id, |r| {
                    r.fail(ferrum_domain::DbError::new(
                        ferrum_domain::DbErrorKind::Internal,
                        e,
                    ));
                });
                return;
            }
        };
        if !matches!(
            state.conn_status.peek().get(&profile.id),
            Some(ConnStatus::Ready(_))
        ) {
            state
                .conn_status
                .write()
                .insert(profile.id, ConnStatus::Ready(profile.engine.label().into()));
        }
        while let Some(envelope) = rx.recv().await {
            // A newer run replaced this one, or the tab closed: drop the stream,
            // which tells the supervisor to cancel server-side work.
            if !with_run(state, tab_id, envelope.execution_id, |r| {
                r.apply(envelope.event)
            }) {
                break;
            }
        }
    });
}

/// Applies `f` if the tab's current run is `execution_id`. Returns false for stale runs.
fn with_run(
    mut state: AppState,
    tab_id: EditorTabId,
    execution_id: ExecutionId,
    f: impl FnOnce(&mut TabRun),
) -> bool {
    let mut runs = state.runs.write();
    match runs.get_mut(&tab_id) {
        Some(run) if run.execution_id == execution_id => {
            f(run);
            true
        }
        _ => false,
    }
}

pub fn cancel_active(state: AppState, services: &AppServices) {
    let Some(id) = *state.active_tab.peek() else {
        return;
    };
    if let Some(run) = state.runs.peek().get(&id)
        && !run.state.is_terminal()
    {
        services.queries.cancel(run.execution_id);
    }
}

//! Running and cancelling queries from the editor.

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{ConnStatus, Dialog, TabRun};
use ferrum_ui::prelude::*;
use ferrumdata_application::safety;
use ferrumdata_domain::{
    ConnectionProfile, DbError, DbErrorKind, EditorTabId, ExecutionId, QueryRequest,
};
use ferrumdata_editor_core::statement_at;
use std::ops::Range;

/// The text to run: the trimmed selection, or the statement under the caret.
/// `selection` is a byte range into `doc`.
#[must_use]
pub fn sql_to_run(doc: &str, selection: Range<usize>) -> String {
    if !selection.is_empty() {
        doc.get(selection)
            .unwrap_or_default()
            .trim()
            .trim_end_matches(';')
            .trim()
            .to_owned()
    } else {
        statement_at(doc, selection.start)
            .map(|s| s.text(doc).to_owned())
            .unwrap_or_default()
    }
}

impl FerrumData {
    /// Runs the selection, or the statement under the caret, of the active tab.
    pub fn run_from_editor(&mut self, cx: &mut Context<Self>) {
        let Some(tab) = self.state.active_tab().cloned() else {
            return;
        };
        let Some(profile) = tab
            .profile_id
            .and_then(|id| self.state.profile(id))
            .cloned()
        else {
            self.notify_user("Choose a connection for this tab first.", true, cx);
            return;
        };
        let inputs = if tab.table.is_some() {
            &self.table_inputs
        } else {
            &self.editors
        };
        let selection = inputs
            .get(&tab.id)
            .map_or(0..0, |entry| entry.input.read(cx).selection());
        let sql = sql_to_run(&tab.sql, selection);
        if sql.is_empty() {
            self.notify_user("Nothing to run.", false, cx);
            return;
        }
        if safety::requires_confirmation(&profile, &sql) {
            self.state.dialog = Some(Dialog::ConfirmRun {
                tab_id: tab.id,
                profile,
                sql,
            });
            cx.notify();
            return;
        }
        self.start_run(tab.id, profile, sql, cx);
    }

    /// Starts a run, superseding (and cancelling) any previous run in the tab.
    pub fn start_run(
        &mut self,
        tab_id: EditorTabId,
        profile: ConnectionProfile,
        sql: String,
        cx: &mut Context<Self>,
    ) {
        let execution_id = ExecutionId::new();
        let limits = self.state.settings.query_limits;
        let run = TabRun::new(execution_id, profile.id, sql.clone(), limits.max_rows);
        if let Some(previous) = self.state.runs.insert(tab_id, run) {
            self.services.queries.cancel(previous.execution_id);
        }
        tracing::info!(execution = %execution_id, profile = %profile.id, "query started");
        cx.notify();

        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let request = QueryRequest {
                execution_id,
                profile_id: profile.id,
                editor_id: Some(tab_id),
                sql,
                limits,
            };
            let queries = services.queries.clone();
            let started =
                background(&services, async move { queries.execute(request).await }).await;
            let mut rx = match started {
                Ok(Ok(rx)) => rx,
                Ok(Err(e)) => {
                    log_dropped(this.update(cx, |this, cx| {
                        this.state
                            .conn_status
                            .insert(profile.id, ConnStatus::Failed(e.message.clone()));
                        this.with_run(tab_id, execution_id, |r| r.fail(e));
                        cx.notify();
                    }));
                    return;
                }
                Err(e) => {
                    log_dropped(this.update(cx, |this, cx| {
                        this.with_run(tab_id, execution_id, |r| {
                            r.fail(DbError::new(DbErrorKind::Internal, e));
                        });
                        cx.notify();
                    }));
                    return;
                }
            };
            log_dropped(this.update(cx, |this, cx| {
                if !matches!(
                    this.state.conn_status.get(&profile.id),
                    Some(ConnStatus::Ready(_))
                ) {
                    this.state
                        .conn_status
                        .insert(profile.id, ConnStatus::Ready(profile.engine.label().into()));
                    cx.notify();
                }
            }));
            while let Some(envelope) = rx.recv().await {
                // A newer run replaced this one, or the tab closed: drop the stream,
                // which tells the supervisor to cancel server-side work.
                let current = this.update(cx, |this, cx| {
                    let current =
                        this.with_run(tab_id, envelope.execution_id, |r| r.apply(envelope.event));
                    cx.notify();
                    current
                });
                if !matches!(current, Ok(true)) {
                    break;
                }
            }
        })
        .detach();
    }

    /// Applies `f` if the tab's current run is `execution_id`. Returns false for stale runs.
    fn with_run(
        &mut self,
        tab_id: EditorTabId,
        execution_id: ExecutionId,
        f: impl FnOnce(&mut TabRun),
    ) -> bool {
        match self.state.runs.get_mut(&tab_id) {
            Some(run) if run.execution_id == execution_id => {
                f(run);
                true
            }
            _ => false,
        }
    }

    pub fn cancel_active(&mut self) {
        let Some(id) = self.state.active_tab else {
            return;
        };
        if let Some(run) = self.state.runs.get(&id)
            && !run.state.is_terminal()
        {
            self.services.queries.cancel(run.execution_id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sql_to_run;

    #[test]
    fn runs_trimmed_selection_without_trailing_semicolon() {
        let doc = "select 1;\nselect 2;";
        assert_eq!(sql_to_run(doc, 10..19), "select 2");
    }

    #[test]
    fn runs_statement_under_caret_when_nothing_selected() {
        let doc = "select 1;\nselect 2;";
        assert_eq!(sql_to_run(doc, 12..12), "select 2");
    }

    #[test]
    fn out_of_bounds_selection_runs_nothing() {
        assert_eq!(sql_to_run("select 1", 3..99), "");
    }
}

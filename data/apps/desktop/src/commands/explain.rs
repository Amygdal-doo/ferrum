//! EXPLAIN of the selection or the statement under the caret, shown in the
//! results panel's Plan pane. ANALYZE always asks first: it runs the statement.

use super::query::sql_to_run;
use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{Dialog, PlanLoad, ResultsPane};
use ferrum_ui::prelude::*;
use ferrumdata_application::explain::run_explain;
use ferrumdata_domain::{ConnectionProfile, DatabaseEngine, EditorTabId};

impl FerrumData {
    /// EXPLAIN is offered for PostgreSQL, MySQL and SQLite SQL tabs.
    pub fn can_explain(&self) -> bool {
        self.state.active_tab().is_some_and(|t| t.tool.is_none())
            && self.state.active_profile().is_some_and(|p| {
                matches!(
                    p.engine,
                    DatabaseEngine::Postgres | DatabaseEngine::MySql | DatabaseEngine::Sqlite
                )
            })
    }

    pub fn explain_from_editor(&mut self, analyze: bool, cx: &mut Context<Self>) {
        let Some(tab) = self.state.active_tab().cloned() else {
            return;
        };
        let Some(profile) = self.state.active_profile().cloned() else {
            self.notify_user("Choose a connection for this tab first.", true, cx);
            return;
        };
        if analyze && profile.engine == DatabaseEngine::Sqlite {
            self.notify_user(
                "SQLite has no EXPLAIN ANALYZE: Explain shows its query plan.",
                true,
                cx,
            );
            return;
        }
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
            self.notify_user("Nothing to explain.", false, cx);
            return;
        }
        if analyze {
            self.state.dialog = Some(Dialog::ConfirmAnalyze {
                tab_id: tab.id,
                profile,
                sql,
            });
            cx.notify();
            return;
        }
        self.start_explain(tab.id, profile, sql, false, cx);
    }

    pub fn start_explain(
        &mut self,
        tab_id: EditorTabId,
        profile: ConnectionProfile,
        sql: String,
        analyze: bool,
        cx: &mut Context<Self>,
    ) {
        self.state
            .plans
            .insert(tab_id, PlanLoad::Loading { analyze });
        self.state.results_pane = ResultsPane::Plan;
        cx.notify();
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let queries = services.queries.clone();
            let statement = sql.clone();
            let result = background(&services, async move {
                run_explain(&queries, profile.engine, profile.id, &statement, analyze).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let load = match result {
                    Ok(Ok(plan)) => PlanLoad::Loaded { plan, sql },
                    Ok(Err(e)) => PlanLoad::Failed(e.message),
                    Err(e) => PlanLoad::Failed(e),
                };
                this.state.plans.insert(tab_id, load);
                // The EXPLAIN run was recorded in the history.
                this.invalidate_history();
                cx.notify();
            }));
        })
        .detach();
    }
}

//! Query history panel: loading, searching, re-opening and re-running.

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{Dialog, EditorTab, HistoryScope, ResultsPane};
use ferrum_ui::prelude::*;
use ferrumdata_application::{HistoryQuery, safety};
use ferrumdata_domain::{HistoryEntry, ProfileId};

/// Entries listed at once; narrow down further with the search box.
pub const HISTORY_PAGE: usize = 200;

impl FerrumData {
    pub fn show_pane(&mut self, pane: ResultsPane, cx: &mut Context<Self>) {
        self.state.results_pane = pane;
        if pane == ResultsPane::History {
            // Always show fresh entries when the panel is opened.
            self.state.history.loaded_for = None;
        }
        cx.notify();
    }

    /// What the panel should list now; `None` when it is scoped to the
    /// active tab's connection and the tab has none.
    #[must_use]
    pub fn history_scope(&self, cx: &App) -> Option<HistoryScope> {
        let profile_id = if self.state.history.all_connections {
            None
        } else {
            Some(self.state.active_profile()?.id)
        };
        Some(HistoryScope {
            profile_id,
            search: self.history_filter.read(cx).text().trim().to_owned(),
        })
    }

    /// Marks the list stale; it reloads on the next render if visible.
    pub fn invalidate_history(&mut self) {
        self.state.history.loaded_for = None;
    }

    /// Reloads if the scope or search changed since the last load. Safe to
    /// call while rendering: it does not notify synchronously.
    pub fn refresh_history_if_stale(&mut self, cx: &mut Context<Self>) {
        let scope = self.history_scope(cx);
        let history = &mut self.state.history;
        if scope.is_none() {
            history.entries.clear();
            history.loading = false;
            history.loaded_for = None;
            return;
        }
        if history.loaded_for == scope {
            return;
        }
        let Some(scope) = scope else { return };
        history.generation += 1;
        history.loading = true;
        history.loaded_for = Some(scope.clone());
        let generation = history.generation;
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let query = HistoryQuery {
                profile_id: scope.profile_id,
                search: scope.search,
                limit: HISTORY_PAGE,
            };
            let result = background(
                &services,
                async move { svc.store.list_history(&query).await },
            )
            .await;
            log_dropped(this.update(cx, |this, cx| {
                let history = &mut this.state.history;
                if history.generation != generation {
                    return;
                }
                history.loading = false;
                match result {
                    Ok(Ok(entries)) => {
                        history.entries = entries;
                        history.error = None;
                    }
                    Ok(Err(e)) => history.error = Some(format!("Could not load history: {e}")),
                    Err(e) => history.error = Some(e),
                }
                cx.notify();
            }));
        })
        .detach();
    }

    /// Opens the entry's query in a new tab bound to its connection, and
    /// optionally runs it (with the usual production confirmation).
    pub fn open_history_entry(&mut self, entry: &HistoryEntry, run: bool, cx: &mut Context<Self>) {
        let profile = self.state.profile(entry.profile_id).cloned();
        let mut tab = EditorTab::new(self.state.tabs.len() + 1, profile.as_ref().map(|p| p.id));
        tab.sql = entry.sql.clone();
        tab.unsaved = true;
        let tab_id = tab.id;
        self.state.tabs.push(tab);
        self.state.active_tab = Some(tab_id);
        self.save_tabs(cx);
        cx.notify();
        if !run {
            return;
        }
        let Some(profile) = profile else {
            self.notify_user("That connection no longer exists.", true, cx);
            return;
        };
        self.state.results_pane = ResultsPane::Results;
        let sql = entry.sql.clone();
        if safety::requires_confirmation(&profile, &sql) {
            self.state.dialog = Some(Dialog::ConfirmRun {
                tab_id,
                profile,
                sql,
            });
            return;
        }
        self.start_run(tab_id, profile, sql, cx);
    }

    /// Deletes one connection's history, or everyone's for `None`.
    pub fn clear_history(&mut self, profile_id: Option<ProfileId>, cx: &mut Context<Self>) {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.store.clear_history(profile_id).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| {
                match result {
                    Ok(Ok(())) => {}
                    Ok(Err(e)) => {
                        this.notify_user(format!("Could not clear history: {e}"), true, cx)
                    }
                    Err(e) => this.notify_user(e, true, cx),
                }
                this.invalidate_history();
                cx.notify();
            }));
        })
        .detach();
    }
}

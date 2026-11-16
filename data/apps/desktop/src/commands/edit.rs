//! Row editing in a table tab: the cell editor, staging, and applying the
//! staged changes as one transaction (`ConnectionService::execute_script`).

use super::{background, log_dropped};
use crate::app::FerrumData;
use crate::state::{Dialog, SchemaLoad};
use ferrum_ui::gpui::ScrollStrategy;
use ferrum_ui::prelude::*;
use ferrumdata_application::ScriptStatement;
use ferrumdata_application::edits::{EditTarget, NewValue};
use ferrumdata_domain::{EditorTabId, QueryState};
use ferrumdata_result_grid_core::{GridSelection, cell_text};

/// A cell of an existing result row (buffer index) or of a staged new row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellTarget {
    Existing { row: usize, col: usize },
    New { index: usize, col: usize },
}

/// The inline editor of one cell.
pub struct CellEditor {
    pub tab_id: EditorTabId,
    pub target: CellTarget,
    pub input: Entity<TextInput>,
    _subscription: Subscription,
}

/// Leading words allowed after `SELECT * FROM <table>` in an editable query.
const EDITABLE_TAILS: [&str; 3] = ["WHERE", "ORDER", "LIMIT"];

impl FerrumData {
    /// Whether the tab's result can be edited, and how; `Err` says why not.
    pub fn edit_target(&self, tab_id: EditorTabId) -> Result<EditTarget, String> {
        let tab = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .ok_or_else(String::new)?;
        let table = tab.table.as_ref().ok_or("Not a table tab")?;
        let profile = tab
            .profile_id
            .and_then(|id| self.state.profile(id))
            .ok_or("No connection")?;
        let run = self.state.runs.get(&tab_id).ok_or_else(String::new)?;
        if run.state != QueryState::Completed {
            return Err("Rows can be edited once they are loaded".into());
        }
        let Some(SchemaLoad::Loaded(tables)) = self.state.schemas.get(&profile.id) else {
            return Err("Loading the table's primary key…".into());
        };
        let info = tables
            .iter()
            .find(|t| &t.table == table)
            .ok_or("The table is no longer listed: reload the connection")?;
        let browse = format!("SELECT * FROM {}", table.qualified(profile.engine));
        if !selects_own_rows(&run.sql, &browse) {
            return Err(format!(
                "Read-only result: only `{browse} [WHERE …] [ORDER BY …] [LIMIT …]` can be edited"
            ));
        }
        let columns: Vec<String> = run
            .buffer
            .columns()
            .iter()
            .map(|c| c.name.clone())
            .collect();
        EditTarget::new(profile.engine, info, &columns).map_err(str::to_owned)
    }

    /// Editing is allowed: an editable result on a writable connection.
    pub fn can_edit(&self, tab_id: EditorTabId) -> bool {
        let read_only = self.state.active_profile().is_none_or(|p| p.read_only);
        !read_only && self.state.active_tab == Some(tab_id) && self.edit_target(tab_id).is_ok()
    }

    pub fn has_pending_edits(&self, tab_id: EditorTabId) -> bool {
        self.state
            .runs
            .get(&tab_id)
            .is_some_and(|r| !r.edits.is_empty())
    }

    /// Double-click: opens the inline editor on a cell (view coordinates).
    pub fn begin_cell_edit(
        &mut self,
        tab_id: EditorTabId,
        view_row: usize,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_edit(tab_id) {
            return;
        }
        let Some(run) = self.state.runs.get(&tab_id) else {
            return;
        };
        let (target, text) = match run.buffer_index(view_row) {
            Some(row) => {
                if run.edits.is_deleted(row) {
                    return;
                }
                let text = match run.edits.value(row, col) {
                    Some(NewValue::Text(t)) => t.clone(),
                    Some(NewValue::Null) => String::new(),
                    None => run
                        .buffer
                        .row(row)
                        .and_then(|r| r.get(col))
                        .filter(|v| !v.is_null())
                        .map(|v| cell_text(v).into_owned())
                        .unwrap_or_default(),
                };
                (CellTarget::Existing { row, col }, text)
            }
            None => {
                let index = view_row - run.buffer.len();
                let text = match run.edits.new_rows().get(index).and_then(|r| r.get(col)) {
                    Some(Some(NewValue::Text(t))) => t.clone(),
                    _ => String::new(),
                };
                (CellTarget::New { index, col }, text)
            }
        };
        self.commit_cell_edit(cx);
        let input = cx.new(|cx| TextInput::new(window, cx).monospace().with_text(text));
        let subscription = cx.subscribe_in(&input, window, |this, _, event, _, cx| match event {
            TextInputEvent::Submit | TextInputEvent::Blurred => this.commit_cell_edit(cx),
            TextInputEvent::Changed => {}
        });
        input.update(cx, |input, cx| {
            let end = input.text().len();
            input.select_range(0..end, cx);
            input.focus(window, cx);
        });
        self.cell_editor = Some(CellEditor {
            tab_id,
            target,
            input,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Stages the open cell editor's text (Enter, or focus moved away).
    /// Typing a cell's original text back drops its change.
    pub fn commit_cell_edit(&mut self, cx: &mut Context<Self>) {
        let Some(editor) = self.cell_editor.take() else {
            return;
        };
        let text = editor.input.read(cx).text().to_owned();
        if let Some(run) = self.state.runs.get_mut(&editor.tab_id) {
            match editor.target {
                CellTarget::Existing { row, col } => {
                    let original = run.buffer.row(row).and_then(|r| r.get(col));
                    let unchanged =
                        original.is_some_and(|v| !v.is_null() && cell_text(v) == text.as_str());
                    if unchanged {
                        run.edits.revert(row, col);
                    } else {
                        run.edits.set(row, col, NewValue::Text(text));
                    }
                }
                CellTarget::New { index, col } => {
                    run.edits.set_new(index, col, Some(NewValue::Text(text)));
                }
            }
        }
        cx.notify();
    }

    /// Escape in the cell editor: closes it without staging.
    pub fn cancel_cell_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.cell_editor.take().is_some() {
            window.focus(&self.grid_focus, cx);
            cx.notify();
        }
    }

    /// "+ Row": stages an empty row (column defaults) and edits its first cell.
    pub fn add_row(&mut self, tab_id: EditorTabId, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(run) = self.state.runs.get_mut(&tab_id) else {
            return;
        };
        let index = run.edits.add_row(run.buffer.columns().len());
        let view_row = run.buffer.len() + index;
        run.selection = Some(GridSelection::cell(view_row, 0));
        self.grid_scroll
            .scroll_to_item(view_row, ScrollStrategy::Center);
        self.begin_cell_edit(tab_id, view_row, 0, window, cx);
    }

    /// Marks the selected rows for deletion (again: unmarks them); staged new
    /// rows in the selection are dropped.
    pub fn delete_selected_rows(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(run) = self.state.runs.get_mut(&tab_id) else {
            return;
        };
        let Some(selection) = run.selection else {
            return;
        };
        let rows = selection.rows();
        let len = run.buffer.len();
        for view_row in rows.clone() {
            if let Some(row) = run.buffer_index(view_row) {
                run.edits.toggle_delete(row);
            }
        }
        // Highest first so the remaining indexes stay valid.
        for view_row in rows.rev().filter(|r| *r >= len) {
            run.edits.remove_new(view_row - len);
        }
        cx.notify();
    }

    /// Sets the selected cell to NULL (staged).
    pub fn set_selected_null(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.cell_editor = None;
        let Some(run) = self.state.runs.get_mut(&tab_id) else {
            return;
        };
        let Some(selection) = run.selection.filter(|s| s.is_single_cell()) else {
            return;
        };
        let (view_row, col) = (selection.anchor.row, selection.anchor.col);
        match run.buffer_index(view_row) {
            Some(row) => {
                let was_null = run
                    .buffer
                    .row(row)
                    .and_then(|r| r.get(col))
                    .is_some_and(|v| v.is_null());
                if was_null {
                    run.edits.revert(row, col);
                } else {
                    run.edits.set(row, col, NewValue::Null);
                }
            }
            None => {
                let index = view_row - run.buffer.len();
                run.edits.set_new(index, col, Some(NewValue::Null));
            }
        }
        cx.notify();
    }

    pub fn discard_edits(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.cell_editor = None;
        if let Some(run) = self.state.runs.get_mut(&tab_id) {
            run.edits = Default::default();
            cx.notify();
        }
    }

    /// The staged changes as statements, or why they cannot be written.
    pub fn edit_script(&self, tab_id: EditorTabId) -> Result<Vec<ScriptStatement>, String> {
        let target = self.edit_target(tab_id)?;
        let run = self.state.runs.get(&tab_id).ok_or_else(String::new)?;
        run.edits.statements(&target, |i| run.buffer.row(i))
    }

    /// Apply: confirms first on production connections.
    pub fn request_apply(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        self.commit_cell_edit(cx);
        let Some(profile) = self.state.active_profile().cloned() else {
            return;
        };
        let script = match self.edit_script(tab_id) {
            Ok(script) => script,
            Err(e) => {
                self.notify_user(e, true, cx);
                return;
            }
        };
        if profile.env.is_production() {
            let sql = script_text(&script);
            self.state.dialog = Some(Dialog::ConfirmApply {
                tab_id,
                profile,
                sql,
            });
            cx.notify();
            return;
        }
        self.apply_edits(tab_id, cx);
    }

    /// Runs the staged changes in one transaction, then reloads the rows.
    /// On failure nothing changed and the edits stay staged.
    pub fn apply_edits(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let script = match self.edit_script(tab_id) {
            Ok(script) => script,
            Err(e) => {
                self.notify_user(e, true, cx);
                return;
            }
        };
        let Some(profile_id) = self.state.active_profile().map(|p| p.id) else {
            return;
        };
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let svc = services.clone();
            let result = background(&services, async move {
                svc.connections.execute_script(profile_id, script).await
            })
            .await;
            log_dropped(this.update(cx, |this, cx| match result {
                Ok(Ok(counts)) => {
                    if let Some(run) = this.state.runs.get_mut(&tab_id) {
                        run.edits = Default::default();
                    }
                    let n = counts.len();
                    this.notify_user(
                        format!("Applied {n} change{}.", if n == 1 { "" } else { "s" }),
                        false,
                        cx,
                    );
                    if this.state.active_tab == Some(tab_id) {
                        this.refresh_table(cx);
                    }
                }
                Ok(Err(e)) => this.notify_user(e.message, true, cx),
                Err(e) => this.notify_user(e, true, cx),
            }));
        })
        .detach();
    }
}

/// `sql` is the table's own browse query (`SELECT * FROM <table>`), at most
/// filtered, ordered or limited, so every result row is one table row.
#[must_use]
pub fn selects_own_rows(sql: &str, browse: &str) -> bool {
    let sql = sql.trim();
    let Some(tail) = sql
        .get(..browse.len())
        .filter(|head| head.eq_ignore_ascii_case(browse))
        .map(|_| sql[browse.len()..].trim_start())
    else {
        return false;
    };
    tail.is_empty()
        || EDITABLE_TAILS.iter().any(|w| {
            tail.get(..w.len())
                .is_some_and(|h| h.eq_ignore_ascii_case(w))
                && tail[w.len()..].starts_with(char::is_whitespace)
        })
}

/// The script as shown before applying: one statement per line.
#[must_use]
pub fn script_text(script: &[ScriptStatement]) -> String {
    script
        .iter()
        .map(|s| format!("{};", s.sql))
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use super::selects_own_rows;

    #[test]
    fn only_the_tables_own_rows_are_editable() {
        let browse = r#"SELECT * FROM "public"."User""#;
        assert!(selects_own_rows(browse, browse));
        assert!(selects_own_rows(
            r#"select * from "public"."User" where id > 3 order by id LIMIT 5"#,
            browse
        ));
        assert!(!selects_own_rows(
            r#"SELECT * FROM "public"."User" u JOIN x ON true"#,
            browse
        ));
        assert!(!selects_own_rows(
            r#"SELECT * FROM "public"."User2""#,
            browse
        ));
        assert!(!selects_own_rows(
            r#"SELECT * FROM "public"."User" WHEREVER"#,
            browse
        ));
        assert!(!selects_own_rows("SELECT id FROM x", browse));
    }
}

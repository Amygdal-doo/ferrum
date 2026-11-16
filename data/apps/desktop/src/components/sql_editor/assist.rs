//! Syntax highlighting and the completion popup of the SQL editor. The
//! tokenizer and completion engine live in `ferrumdata-editor-core`; this
//! file maps them onto the `TextInput` and the loaded schema.

use crate::app::FerrumData;
use crate::state::{DetailsLoad, SchemaLoad};
use ferrum_ui::gpui::{Anchor, HighlightStyle, MouseButton, anchored, deferred, point};
use ferrum_ui::prelude::*;
use ferrumdata_domain::{DatabaseEngine, EditorTabId, ProfileId, TableRef};
use ferrumdata_editor_core::{
    Catalog, CatalogTable, Completions, Dialect, complete, highlight_spans, mentions_at,
};

actions!(sql_editor, [ShowCompletions]);

pub(crate) const KEY_CONTEXT: &str = "SqlEditor";
/// Rows of the popup shown at once; the list scrolls with the selection.
const VISIBLE_ITEMS: usize = 10;

/// ⌃Space / ⌘I open the completion list on demand.
pub fn bind_editor_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("ctrl-space", ShowCompletions, context),
        KeyBinding::new("cmd-i", ShowCompletions, context),
    ]);
}

/// The open completion list of one tab, valid while the caret stays put.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompletionPopup {
    pub tab_id: EditorTabId,
    pub caret: usize,
    pub selected: usize,
}

/// Lexer rules for an engine's editor; `None` for non-SQL input.
#[must_use]
pub fn dialect(engine: Option<DatabaseEngine>) -> Option<Dialect> {
    match engine {
        None | Some(DatabaseEngine::Postgres) => Some(Dialect::Postgres),
        // SurrealQL quotes like MySQL (backticks, `#` comments).
        Some(DatabaseEngine::MySql | DatabaseEngine::SurrealDb) => Some(Dialect::MySql),
        Some(DatabaseEngine::Sqlite) => Some(Dialect::Sqlite),
        Some(DatabaseEngine::MongoDb | DatabaseEngine::Redis) => None,
    }
}

/// Completion needs a SQL catalog: PostgreSQL, MySQL and SQLite.
fn completes(engine: Option<DatabaseEngine>) -> bool {
    matches!(
        engine,
        Some(DatabaseEngine::Postgres | DatabaseEngine::MySql | DatabaseEngine::Sqlite)
    )
}

/// Colours `input` with the theme's syntax styles (cleared for non-SQL).
pub fn apply_highlights(input: &Entity<TextInput>, engine: Option<DatabaseEngine>, cx: &mut App) {
    let syntax = cx.theme().syntax().clone();
    input.update(cx, |input, cx| {
        let spans: Vec<(std::ops::Range<usize>, HighlightStyle)> = dialect(engine)
            .map(|d| highlight_spans(input.text(), d))
            .unwrap_or_default()
            .into_iter()
            .filter_map(|(range, scope)| Some((range, syntax.style_for_name(scope)?)))
            .collect();
        input.set_highlights(spans, cx);
    });
}

impl FerrumData {
    pub(crate) fn tab_engine(
        &self,
        tab_id: EditorTabId,
    ) -> (Option<ProfileId>, Option<DatabaseEngine>) {
        let profile = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.profile_id);
        let engine = profile
            .and_then(|id| self.state.profile(id))
            .map(|p| p.engine);
        (profile, engine)
    }

    /// After an edit: re-highlight, then open the list while a word or a
    /// `.` is being typed, and close it otherwise.
    pub(crate) fn editor_changed(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(input) = self.editors.get(&tab_id).map(|e| e.input.clone()) else {
            return;
        };
        let (_, engine) = self.tab_engine(tab_id);
        apply_highlights(&input, engine, cx);
        if std::mem::take(&mut self.accepting_completion) {
            self.completion = None;
            return;
        }
        let (typing, caret) = {
            let input = input.read(cx);
            let caret = input.cursor();
            let before = input.text()[..caret].chars().next_back();
            let typing = before.is_some_and(|c| c == '_' || c == '.' || c.is_alphanumeric());
            (typing, caret)
        };
        if typing && completes(engine) {
            self.open_completion(tab_id, caret, cx);
        } else {
            self.completion = None;
        }
    }

    pub(crate) fn show_completions(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        let Some(input) = self.editors.get(&tab_id).map(|e| e.input.clone()) else {
            return;
        };
        if completes(self.tab_engine(tab_id).1) {
            let caret = input.read(cx).cursor();
            self.open_completion(tab_id, caret, cx);
        }
    }

    fn open_completion(&mut self, tab_id: EditorTabId, caret: usize, cx: &mut Context<Self>) {
        let selected = self
            .completion
            .filter(|p| p.tab_id == tab_id)
            .map_or(0, |p| p.selected);
        self.completion = Some(CompletionPopup {
            tab_id,
            caret,
            selected,
        });
        self.load_completion_schema(tab_id, caret, cx);
        cx.notify();
    }

    /// Loads the table list, and the columns of the tables the statement
    /// names, when they are not loaded yet. The popup fills in as they arrive.
    fn load_completion_schema(
        &mut self,
        tab_id: EditorTabId,
        caret: usize,
        cx: &mut Context<Self>,
    ) {
        let (Some(profile_id), engine) = self.tab_engine(tab_id) else {
            return;
        };
        let Some(dialect) = dialect(engine) else {
            return;
        };
        match self.state.schemas.get(&profile_id) {
            Some(SchemaLoad::Loaded(_)) => {}
            Some(SchemaLoad::Loading) => return,
            _ => {
                self.load_tables(profile_id, cx);
                return;
            }
        }
        let sql = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .map(|t| t.sql.clone())
            .unwrap_or_default();
        let catalog = self.catalog(profile_id);
        for mention in mentions_at(&sql, caret, dialect) {
            let Some(table) = catalog.find(mention.schema.as_deref(), &mention.name) else {
                continue;
            };
            let key = TableRef {
                schema: table.schema.clone(),
                name: table.name.clone(),
            };
            if !self
                .state
                .table_details
                .contains_key(&(profile_id, key.clone()))
            {
                self.load_table_details(profile_id, key, cx);
            }
        }
    }

    /// Tables of the loaded schema, with columns where they are loaded.
    fn catalog(&self, profile_id: ProfileId) -> Catalog {
        let Some(SchemaLoad::Loaded(tables)) = self.state.schemas.get(&profile_id) else {
            return Catalog::default();
        };
        let tables = tables
            .iter()
            .map(|info| {
                let columns = match self
                    .state
                    .table_details
                    .get(&(profile_id, info.table.clone()))
                {
                    Some(DetailsLoad::Loaded(details)) => {
                        Some(details.columns.iter().map(|c| c.name.clone()).collect())
                    }
                    _ => None,
                };
                CatalogTable {
                    schema: info.table.schema.clone(),
                    name: info.table.name.clone(),
                    columns,
                }
            })
            .collect();
        Catalog { tables }
    }

    /// The popup's items, or `None` (and the popup closed) when the caret
    /// moved away or there is nothing to offer.
    fn completion_items(&mut self, cx: &App) -> Option<(CompletionPopup, Completions)> {
        let popup = self.completion?;
        let input = self.editors.get(&popup.tab_id)?.input.read(cx);
        let (profile, engine) = self.tab_engine(popup.tab_id);
        let fresh = self.state.active_tab == Some(popup.tab_id)
            && input.cursor() == popup.caret
            && input.selection().is_empty();
        let items = fresh
            .then(|| {
                complete(
                    input.text(),
                    popup.caret,
                    dialect(engine)?,
                    &self.catalog(profile?),
                )
            })
            .flatten()
            .filter(|c| !c.items.is_empty());
        match items {
            Some(items) => Some((popup, items)),
            None => {
                self.completion = None;
                None
            }
        }
    }

    /// ↑ / ↓ while the list is open; `false` lets the editor move the caret.
    pub(crate) fn move_completion(&mut self, delta: isize, cx: &mut Context<Self>) -> bool {
        let Some((popup, items)) = self.completion_items(cx) else {
            return false;
        };
        let last = items.items.len() - 1;
        let selected = popup
            .selected
            .min(last)
            .saturating_add_signed(delta)
            .min(last);
        self.completion = Some(CompletionPopup { selected, ..popup });
        cx.notify();
        true
    }

    /// Enter / Tab / click: replaces the partial word with the item.
    pub(crate) fn accept_completion(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((popup, items)) = self.completion_items(cx) else {
            return false;
        };
        let index = index.unwrap_or(popup.selected).min(items.items.len() - 1);
        let insert = items.items[index].insert.clone();
        let Some(input) = self.editors.get(&popup.tab_id).map(|e| e.input.clone()) else {
            return false;
        };
        self.accepting_completion = true;
        input.update(cx, |input, cx| {
            input.select_range(items.replace.clone(), cx);
            input.insert(&insert, window, cx);
            input.focus(window, cx);
        });
        self.completion = None;
        cx.notify();
        true
    }

    pub(crate) fn dismiss_completion(&mut self, cx: &mut Context<Self>) -> bool {
        if self.completion.take().is_some() {
            cx.notify();
            true
        } else {
            false
        }
    }

    /// The popup under the caret, drawn above everything else.
    pub(crate) fn render_completion(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (popup, completions) = self.completion_items(cx)?;
        let bounds = self
            .editors
            .get(&popup.tab_id)?
            .input
            .read(cx)
            .caret_bounds()?;
        let colors = cx.theme().colors().clone();
        let selected = popup.selected.min(completions.items.len() - 1);
        let first = selected.saturating_sub(VISIBLE_ITEMS - 1);
        let rows = completions
            .items
            .iter()
            .enumerate()
            .skip(first)
            .take(VISIBLE_ITEMS)
            .map(|(i, item)| {
                h_flex()
                    .id(SharedString::from(format!("completion-{i}")))
                    .gap_2()
                    .px_2()
                    .py_0p5()
                    .rounded_sm()
                    .when(i == selected, |row| row.bg(colors.element_selected))
                    .hover(|row| row.bg(colors.element_hover))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.accept_completion(Some(i), window, cx);
                        }),
                    )
                    .child(
                        div()
                            .flex_none()
                            .w(px(56.))
                            .text_color(colors.text_muted)
                            .child(item.kind.label()),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .child(item.label.clone()),
                    )
                    .when_some(item.detail.clone(), |row, detail| {
                        row.child(
                            div()
                                .flex_none()
                                .max_w(px(120.))
                                .truncate()
                                .text_color(colors.text_muted)
                                .child(detail),
                        )
                    })
            });
        let more = completions
            .items
            .len()
            .saturating_sub(first + VISIBLE_ITEMS);
        let list = v_flex()
            .w(px(340.))
            .p_1()
            .rounded_md()
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .shadow_md()
            .font_buffer(cx)
            .text_xs()
            .children(rows)
            .when(more > 0, |list| {
                list.child(
                    div()
                        .px_2()
                        .text_color(colors.text_muted)
                        .child(format!("{more} more…")),
                )
            });
        Some(
            deferred(
                anchored()
                    .anchor(Anchor::TopLeft)
                    .position(point(bounds.left(), bounds.bottom()))
                    .snap_to_window_with_margin(px(8.))
                    .child(list),
            )
            .with_priority(1)
            .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_engines_complete_and_document_engines_do_not() {
        assert!(completes(Some(DatabaseEngine::Sqlite)));
        assert!(!completes(Some(DatabaseEngine::MongoDb)));
        assert!(!completes(None));
        assert_eq!(dialect(Some(DatabaseEngine::Redis)), None);
        assert_eq!(dialect(None), Some(Dialect::Postgres));
    }
}

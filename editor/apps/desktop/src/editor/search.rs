//! In-buffer search state driven by the find bar: matches, the active
//! match, replace one / replace all.

use std::ops::Range;

use ferrumeditor_core::editor::EditorState;
use ferrumeditor_core::find::{CompiledQuery, SearchQuery, next_match_index, previous_match_index};
use ferrumeditor_core::selection::Selection;
use gpui::Context;

use super::{EditorEvent, EditorView};

#[derive(Debug, Default, Clone)]
pub struct SearchState {
    pub query: Option<SearchQuery>,
    compiled: Option<CompiledQuery>,
    pub matches: Vec<Range<usize>>,
    pub active: Option<usize>,
    pub error: Option<String>,
    version: Option<u64>,
}

impl SearchState {
    fn set_query(&mut self, query: Option<SearchQuery>, state: &EditorState) {
        self.error = None;
        self.compiled = None;
        self.matches.clear();
        self.active = None;
        self.version = None;
        if let Some(query) = &query {
            match query.compile() {
                Ok(compiled) => self.compiled = Some(compiled),
                Err(ferrumeditor_core::find::FindError::Empty) => {}
                Err(error) => self.error = Some(error.to_string()),
            }
        }
        self.query = query;
        self.refresh(state);
    }

    /// Recomputes matches if the buffer changed.
    pub fn refresh(&mut self, state: &EditorState) {
        let Some(compiled) = &self.compiled else {
            return;
        };
        if self.version == Some(state.buffer().version()) {
            return;
        }
        self.version = Some(state.buffer().version());
        self.matches = compiled.find_all(&state.text());
        self.active = self.active.filter(|i| *i < self.matches.len());
    }

    pub fn is_active(&self) -> bool {
        self.compiled.is_some()
    }
}

impl EditorView {
    /// Sets (or clears) the find-bar query and selects the first match at
    /// or after the selection start.
    pub fn set_search(&mut self, query: Option<SearchQuery>, select: bool, cx: &mut Context<Self>) {
        self.search.set_query(query, &self.state);
        if select {
            let from = self.state.primary().start();
            if let Some(index) = next_match_index(&self.search.matches, from) {
                self.select_match(index, cx);
            }
        }
        cx.notify();
    }

    pub fn clear_search(&mut self, cx: &mut Context<Self>) {
        self.search.set_query(None, &self.state);
        cx.notify();
    }

    fn select_match(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(range) = self.search.matches.get(index).cloned() else {
            return;
        };
        self.search.active = Some(index);
        self.state
            .set_selections(vec![Selection::new(range.start, range.end)]);
        self.request_autoscroll();
        cx.emit(EditorEvent::SelectionsChanged);
        cx.notify();
    }

    /// Selects the next (or previous) match; false when there is none.
    pub fn find_next(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        self.search.refresh(&self.state);
        let primary = self.state.primary();
        let index = if forward {
            next_match_index(&self.search.matches, primary.end().max(primary.start() + 1))
                .or_else(|| next_match_index(&self.search.matches, 0))
        } else {
            previous_match_index(&self.search.matches, primary.start())
        };
        match index {
            Some(index) => {
                self.select_match(index, cx);
                true
            }
            None => false,
        }
    }

    /// Replaces the selected match (if the selection is one) and moves on.
    pub fn replace_next(&mut self, replacement: &str, cx: &mut Context<Self>) {
        self.search.refresh(&self.state);
        let primary = self.state.primary().range();
        if let (Some(compiled), Some(_)) = (
            self.search.compiled.clone(),
            self.search.matches.iter().position(|m| *m == primary),
        ) {
            let text = self.state.text();
            let new = compiled.expand_replacement(&text, primary.clone(), replacement);
            self.state.replace_ranges(vec![(primary, new)]);
            self.after_edit(cx);
        }
        self.find_next(true, cx);
    }

    /// Replaces every match as one undo step; returns the count.
    pub fn replace_all(&mut self, replacement: &str, cx: &mut Context<Self>) -> usize {
        self.search.refresh(&self.state);
        let Some(compiled) = self.search.compiled.clone() else {
            return 0;
        };
        let text = self.state.text();
        let edits: Vec<(Range<usize>, String)> = self
            .search
            .matches
            .iter()
            .map(|m| {
                (
                    m.clone(),
                    compiled.expand_replacement(&text, m.clone(), replacement),
                )
            })
            .collect();
        let count = edits.len();
        if count > 0 {
            self.state.replace_ranges(edits);
            self.after_edit(cx);
        }
        count
    }

    /// Turns every match into a selection.
    pub fn select_all_matches(&mut self, cx: &mut Context<Self>) {
        self.search.refresh(&self.state);
        if self.search.matches.is_empty() {
            return;
        }
        let selections = self
            .search
            .matches
            .iter()
            .map(|m| Selection::new(m.start, m.end))
            .collect();
        self.state.set_selections(selections);
        self.request_autoscroll();
        cx.emit(EditorEvent::SelectionsChanged);
        cx.notify();
    }

    /// The primary selection's text, for "use selection as find".
    pub fn selected_text(&self) -> Option<String> {
        let primary = self.state.primary();
        (!primary.is_empty()).then(|| self.state.buffer().slice(primary.range()))
    }

    pub fn search_summary(&self) -> (Option<usize>, usize) {
        (self.search.active, self.search.matches.len())
    }
}

//! Snippet expansion on Tab and the tab-stop session that follows it.

use std::ops::Range;

use super::{Change, EditorState};
use crate::history::EditKind;
use crate::selection::{Selection, transform_offset};
use crate::snippets::{Snippet, expand};
use crate::text::{CharClass, char_class};

/// Live tab stops of an expanded snippet: for each stop (in visiting
/// order) one range per cursor that expanded it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnippetSession {
    stops: Vec<Vec<Range<usize>>>,
    current: usize,
}

impl SnippetSession {
    pub fn current_ranges(&self) -> &[Range<usize>] {
        self.stops.get(self.current).map_or(&[], |s| &s[..])
    }

    pub(crate) fn transform(&mut self, range: &Range<usize>, new_len: usize) {
        for stop in &mut self.stops {
            for r in stop.iter_mut() {
                let start = transform_offset(r.start, range, new_len, false);
                let end = transform_offset(r.end, range, new_len, true).max(start);
                *r = start..end;
            }
        }
    }

    fn contains(&self, offset: usize) -> bool {
        self.current_ranges()
            .iter()
            .any(|r| r.start <= offset && offset <= r.end)
    }
}

/// The snippet prefix typed before `offset`, if any snippet matches.
fn matching_snippet<'a>(
    state: &EditorState,
    offset: usize,
    snippets: &'a [Snippet],
) -> Option<&'a Snippet> {
    let buffer = state.buffer();
    let line_start = buffer.line_start(buffer.row_of(offset));
    let before = buffer.slice(line_start..offset);
    let run_start = before
        .char_indices()
        .rev()
        .find(|(_, c)| c.is_whitespace())
        .map_or(0, |(i, c)| i + c.len_utf8());
    let run = &before[run_start..];
    snippets
        .iter()
        .filter(|s| !s.prefix.is_empty() && run.ends_with(&s.prefix))
        .filter(|s| {
            let cut = run.len() - s.prefix.len();
            let first_is_word = s
                .prefix
                .chars()
                .next()
                .is_some_and(|c| char_class(c) == CharClass::Word);
            cut == 0
                || !first_is_word
                || run[..cut]
                    .chars()
                    .last()
                    .is_some_and(|c| char_class(c) != CharClass::Word)
        })
        .max_by_key(|s| s.prefix.len())
}

impl EditorState {
    /// Expands the snippet whose prefix ends at every caret. Returns false
    /// (doing nothing) unless all carets match the same snippet.
    pub fn expand_snippet(&mut self, snippets: &[Snippet]) -> bool {
        if snippets.is_empty() || self.selections().iter().any(|s| !s.is_empty()) {
            return false;
        }
        let Some(snippet) = matching_snippet(self, self.primary().head, snippets) else {
            return false;
        };
        let all_match = self.selections().iter().all(|s| {
            matching_snippet(self, s.head, snippets).map(|m| &m.prefix) == Some(&snippet.prefix)
        });
        if !all_match {
            return false;
        }
        let prefix_len = snippet.prefix.len();
        let mut changes = Vec::new();
        let mut expansions = Vec::new();
        for s in self.selections() {
            let row = self.buffer().row_of(s.head);
            let expansion = expand(&snippet.body, &self.buffer().indent_of(row));
            changes.push(Change::new(
                s.head - prefix_len..s.head,
                expansion.text.clone(),
            ));
            expansions.push(expansion);
        }
        // Where each expansion starts after all edits (selections are sorted).
        let mut delta: isize = 0;
        let mut starts = Vec::new();
        for (change, expansion) in changes.iter().zip(&expansions) {
            starts.push((change.range.start as isize + delta) as usize);
            delta += expansion.text.len() as isize - change.range.len() as isize;
        }
        self.snippet = None;
        self.apply_transforming(changes, EditKind::Other);
        let order: Vec<u32> = {
            let mut seen = Vec::new();
            for stop in &expansions[0].stops {
                if !seen.contains(&stop.index) {
                    seen.push(stop.index);
                }
            }
            seen
        };
        let stops: Vec<Vec<Range<usize>>> = order
            .iter()
            .map(|index| {
                expansions
                    .iter()
                    .zip(&starts)
                    .flat_map(|(e, start)| {
                        e.stops
                            .iter()
                            .filter(|s| s.index == *index)
                            .map(move |s| start + s.range.start..start + s.range.end)
                    })
                    .collect()
            })
            .collect();
        self.snippet = Some(SnippetSession { stops, current: 0 });
        self.select_current_stop();
        true
    }

    fn select_current_stop(&mut self) {
        let Some(session) = &self.snippet else { return };
        let selections: Vec<Selection> = session
            .current_ranges()
            .iter()
            .map(|r| Selection::new(r.start, r.end))
            .collect();
        let finished = session.current + 1 >= session.stops.len();
        if !selections.is_empty() {
            self.replace_selections_after_edit(selections);
        }
        if finished {
            self.snippet = None;
        }
    }

    /// Tab inside a snippet: go to the next stop. False when no session is
    /// active at the caret.
    pub fn next_tab_stop(&mut self) -> bool {
        self.step_tab_stop(1)
    }

    pub fn previous_tab_stop(&mut self) -> bool {
        self.step_tab_stop(-1)
    }

    fn step_tab_stop(&mut self, step: isize) -> bool {
        let head = self.primary().head;
        let Some(session) = &mut self.snippet else {
            return false;
        };
        if !session.contains(head) {
            self.snippet = None;
            return false;
        }
        let target = session.current as isize + step;
        if target < 0 {
            return true;
        }
        session.current = (target as usize).min(session.stops.len() - 1);
        self.select_current_stop();
        true
    }

    pub fn end_snippet(&mut self) {
        self.snippet = None;
    }
}

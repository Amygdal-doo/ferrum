//! [`EditorState`]: a buffer, its multi-cursor selections and undo history,
//! plus every editing command as a method. Commands apply all their
//! changes as one transaction so undo restores text and cursors together.
//!
//! The command methods are split by topic: `edit` (typing, deleting,
//! clipboard, indentation), `motion` (cursor movement and selection),
//! `lines` (line-wise commands and comments), `multi` (multi-cursor
//! commands) and `snippet` (snippet expansion and tab stops).

mod apply;
mod brackets;
mod clipboard;
mod comment;
mod edit;
mod indent;
mod lines;
mod motion;
mod multi;
mod replace;
mod snippet;

#[cfg(test)]
pub(crate) mod tests;

use std::ops::Range;
use std::sync::OnceLock;
use std::time::Instant;

pub use brackets::matching_bracket;
pub use clipboard::ClipboardText;
pub use snippet::SnippetSession;

use crate::buffer::Buffer;
use crate::history::History;
use crate::language::Language;
use crate::selection::{Selection, normalize};

/// Per-buffer editing settings (from `config.toml`, possibly per language).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditSettings {
    pub tab_size: usize,
    /// Insert spaces for Tab / indentation.
    pub soft_tabs: bool,
    pub auto_close: bool,
    pub auto_indent: bool,
}

impl Default for EditSettings {
    fn default() -> Self {
        Self {
            tab_size: 4,
            soft_tabs: true,
            auto_close: true,
            auto_indent: true,
        }
    }
}

impl EditSettings {
    /// One level of indentation.
    pub fn indent_unit(&self) -> String {
        if self.soft_tabs {
            " ".repeat(self.tab_size.max(1))
        } else {
            "\t".to_string()
        }
    }
}

/// One replacement planned by a command, in current-text coordinates.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Change {
    pub range: Range<usize>,
    pub text: String,
}

impl Change {
    pub fn new(range: Range<usize>, text: impl Into<String>) -> Self {
        Self {
            range,
            text: text.into(),
        }
    }

    pub fn insert(at: usize, text: impl Into<String>) -> Self {
        Self::new(at..at, text)
    }
}

/// A selection end expressed against a change: `delta` bytes after where
/// the change's new text starts once all changes are applied.
#[derive(Debug, Clone, Copy)]
pub(crate) struct Mark {
    pub change: usize,
    pub delta: usize,
}

/// What a command does at one selection.
pub(crate) struct Plan {
    pub changes: Vec<Change>,
    pub anchor: Mark,
    pub head: Mark,
}

impl Plan {
    /// Replace `range` with `text`, caret `caret` bytes into the new text.
    pub fn replace(range: Range<usize>, text: impl Into<String>, caret: usize) -> Self {
        let mark = Mark {
            change: 0,
            delta: caret,
        };
        Self {
            changes: vec![Change::new(range, text)],
            anchor: mark,
            head: mark,
        }
    }

    /// No edit; the caret moves to `offset`.
    pub fn caret(offset: usize) -> Self {
        Self::replace(offset..offset, "", 0)
    }
}

fn now_ms() -> u64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_millis() as u64
}

#[derive(Debug, Clone)]
pub struct EditorState {
    buffer: Buffer,
    selections: Vec<Selection>,
    primary: usize,
    history: History,
    pub language: Language,
    pub settings: EditSettings,
    snippet: Option<SnippetSession>,
    /// The last Cmd+D started from a caret, so further matches are whole words.
    select_next_whole_word: bool,
    /// Whether the last edit command recorded an undo entry.
    last_edit_recorded: bool,
}

impl EditorState {
    pub fn new(text: &str, language: Language, settings: EditSettings) -> Self {
        Self {
            buffer: Buffer::new(text),
            selections: vec![Selection::caret(0)],
            primary: 0,
            history: History::default(),
            language,
            settings,
            snippet: None,
            select_next_whole_word: false,
            last_edit_recorded: false,
        }
    }

    pub fn buffer(&self) -> &Buffer {
        &self.buffer
    }

    pub fn text(&self) -> String {
        self.buffer.text()
    }

    pub fn selections(&self) -> &[Selection] {
        &self.selections
    }

    pub fn primary_index(&self) -> usize {
        self.primary.min(self.selections.len() - 1)
    }

    pub fn primary(&self) -> Selection {
        self.selections[self.primary_index()]
    }

    pub fn is_dirty(&self) -> bool {
        self.history.is_dirty()
    }

    pub fn mark_saved(&mut self) {
        self.history.mark_saved();
    }

    pub fn can_undo(&self) -> bool {
        self.history.can_undo()
    }

    pub fn snippet_session(&self) -> Option<&SnippetSession> {
        self.snippet.as_ref()
    }

    /// Replaces all selections (e.g. from the mouse or find). Clamps to the
    /// buffer and makes the last one primary.
    pub fn set_selections(&mut self, selections: Vec<Selection>) {
        let len = self.buffer.len();
        let mut selections: Vec<Selection> = selections
            .into_iter()
            .map(|s| Selection {
                anchor: self.buffer.clip(s.anchor.min(len)),
                head: self.buffer.clip(s.head.min(len)),
                goal: s.goal,
            })
            .collect();
        let primary = selections.len().saturating_sub(1);
        self.primary = normalize(&mut selections, primary);
        self.selections = selections;
        self.history.seal();
        self.select_next_whole_word = false;
    }

    pub fn set_cursor(&mut self, offset: usize) {
        self.set_selections(vec![Selection::caret(offset)]);
        self.snippet = None;
    }

    /// The selected texts, one per selection.
    pub fn selected_texts(&self) -> Vec<String> {
        self.selections
            .iter()
            .map(|s| self.buffer.slice(s.range()))
            .collect()
    }

    pub fn undo(&mut self) -> bool {
        self.snippet = None;
        match self.history.undo(&mut self.buffer) {
            Some(selections) => {
                self.restore_selections(selections);
                true
            }
            None => false,
        }
    }

    pub fn redo(&mut self) -> bool {
        self.snippet = None;
        match self.history.redo(&mut self.buffer) {
            Some(selections) => {
                self.restore_selections(selections);
                true
            }
            None => false,
        }
    }

    fn restore_selections(&mut self, selections: Vec<Selection>) {
        let len = self.buffer.len();
        let mut selections: Vec<Selection> = selections
            .into_iter()
            .map(|s| Selection::new(s.anchor.min(len), s.head.min(len)))
            .collect();
        let primary = selections.len().saturating_sub(1);
        self.primary = normalize(&mut selections, primary);
        self.selections = selections;
    }
}

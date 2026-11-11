//! Autocomplete: asking the language server (and falling back to words in
//! the file) as you type or on Ctrl+Space, filtering the menu as the word
//! grows, and inserting the chosen item (snippets and auto-imports too).

use std::collections::HashSet;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use ferrumeditor_core::completion::{
    Candidate, buffer_words, is_word_char, normalize_lsp_snippet, rank, word_start,
};
use ferrumeditor_core::history::EditKind;
use ferrumeditor_core::language::Language;
use ferrumeditor_core::lsp::position::range_to_offsets;
use ferrumeditor_core::lsp::types::{kind_label, parse_completion};
use ferrumeditor_core::lsp::{CompletionItem, LspRange, PositionEncoding, TextEdit};
use gpui::Context;
use serde_json::json;

use super::{EditorEvent, EditorView};
use crate::settings::Settings;

/// Typing waits this long before asking, so fast typing sends one request.
const REQUEST_DEBOUNCE: Duration = Duration::from_millis(40);
/// Buffers larger than this don't offer their own words.
const WORDS_LIMIT: usize = 2 * 1024 * 1024;
const MAX_WORDS: usize = 200;
/// Rows visible at once.
pub const MENU_ROWS: usize = 10;
/// Longest wait for `completionItem/resolve` before inserting as is.
const RESOLVE_TIMEOUT: Duration = Duration::from_millis(400);

#[derive(Debug, Clone)]
pub struct CompletionEntry {
    pub label: String,
    pub kind: &'static str,
    pub detail: Option<String>,
    pub documentation: Option<String>,
    insert: String,
    snippet: bool,
    filter: String,
    sort: String,
    edit_range: Option<LspRange>,
    additional: Vec<TextEdit>,
    from_server: bool,
    preselect: bool,
    /// The server's item, for `completionItem/resolve`.
    item: Option<Box<CompletionItem>>,
}

impl CompletionEntry {
    fn from_item(item: CompletionItem) -> Self {
        let original = Box::new(item.clone());
        Self {
            item: Some(original),
            kind: kind_label(item.kind),
            label: item.label,
            detail: item.detail,
            documentation: item.documentation,
            insert: item.insert_text,
            snippet: item.is_snippet,
            filter: item.filter_text,
            sort: item.sort_text,
            edit_range: item.edit_range,
            additional: item.additional_edits,
            from_server: true,
            preselect: item.preselect,
        }
    }

    fn word(word: String) -> Self {
        Self {
            label: word.clone(),
            kind: "word",
            detail: None,
            documentation: None,
            insert: word.clone(),
            snippet: false,
            filter: word.clone(),
            sort: word,
            edit_range: None,
            additional: Vec::new(),
            from_server: false,
            preselect: false,
            item: None,
        }
    }
}

pub struct CompletionMenu {
    /// Start of the word being completed.
    pub anchor: usize,
    pub entries: Arc<Vec<CompletionEntry>>,
    /// Indices into `entries`, best first.
    pub matches: Vec<usize>,
    pub selected: usize,
    /// First visible row.
    pub scroll_top: usize,
    incomplete: bool,
    encoding: PositionEncoding,
}

impl CompletionMenu {
    pub fn selected_entry(&self) -> Option<&CompletionEntry> {
        self.matches
            .get(self.selected)
            .and_then(|i| self.entries.get(*i))
    }

    fn filter(&mut self, query: &str) {
        let candidates: Vec<Candidate> = self
            .entries
            .iter()
            .map(|e| Candidate {
                filter_text: &e.filter,
                sort_text: &e.sort,
                from_server: e.from_server,
                preselect: e.preselect,
            })
            .collect();
        self.matches = rank(query, &candidates);
        self.selected = 0;
        self.scroll_top = 0;
    }

    pub fn select(&mut self, delta: isize) {
        let count = self.matches.len();
        if count == 0 {
            return;
        }
        let next = (self.selected as isize + delta).rem_euclid(count as isize) as usize;
        self.selected = next;
        if next < self.scroll_top {
            self.scroll_top = next;
        } else if next >= self.scroll_top + MENU_ROWS {
            self.scroll_top = next + 1 - MENU_ROWS;
        }
    }
}

impl EditorView {
    fn trigger_characters(&self) -> Vec<String> {
        self.lsp
            .doc
            .as_ref()
            .and_then(|d| d.client.capabilities())
            .and_then(|c| {
                c["completionProvider"]["triggerCharacters"]
                    .as_array()
                    .map(|chars| {
                        chars
                            .iter()
                            .filter_map(|c| c.as_str().map(str::to_string))
                            .collect()
                    })
            })
            .unwrap_or_default()
    }

    /// Called after plain typing (not IME composition or paste).
    pub(super) fn after_typing(&mut self, typed: &str, cx: &mut Context<Self>) {
        let auto = cx.global::<Settings>().config.autocomplete;
        let mut chars = typed.chars();
        let (Some(last), None) = (chars.next_back(), chars.next_back()) else {
            self.completion = None;
            return;
        };
        if is_word_char(last) {
            if self.completion.is_some() {
                self.refilter_completion(cx);
            } else if auto {
                self.trigger_completion(false, None, cx);
            }
        } else if auto && self.trigger_characters().iter().any(|t| t.ends_with(last)) {
            self.trigger_completion(false, Some(last), cx);
        } else {
            self.completion = None;
            self.completion_task = None;
        }
    }

    /// Asks for completions at the caret. `manual` is Ctrl+Space.
    pub(super) fn trigger_completion(
        &mut self,
        manual: bool,
        trigger: Option<char>,
        cx: &mut Context<Self>,
    ) {
        let primary = self.state.primary();
        if !primary.is_empty() {
            return;
        }
        let head = primary.head;
        let buffer = self.state.buffer();
        let anchor = word_start(buffer, head);
        let prefix = buffer.slice(anchor..head);
        let server = self.lsp.doc.as_ref().filter(|d| {
            !d.client.is_stopped()
                && d.client
                    .capabilities()
                    .is_none_or(|c| !c["completionProvider"].is_null())
        });
        if server.is_none() && manual && prefix.is_empty() {
            cx.emit(EditorEvent::Notice(self.no_server_message(cx)));
            return;
        }
        // Prose gets word suggestions only on request.
        if server.is_none()
            && !manual
            && matches!(
                self.state.language,
                Language::PlainText | Language::Markdown
            )
        {
            return;
        }
        let mut words = Vec::new();
        if (manual || prefix.chars().count() >= 2) && buffer.len() <= WORDS_LIMIT {
            let mut seen = HashSet::new();
            words = buffer_words(&self.state.text(), &prefix, &mut seen, MAX_WORDS);
        }
        let request = server.map(|doc| {
            let context = match trigger {
                Some(c) => json!({ "triggerKind": 2, "triggerCharacter": c.to_string() }),
                None => json!({ "triggerKind": 1 }),
            };
            (doc.client.position_encoding(), doc.client.clone(), context)
        });
        if request.is_some() {
            self.flush_lsp_changes();
        }
        let params = self.position_params(head);
        let debounce = !manual && trigger.is_none();
        self.completion_task = Some(cx.spawn(async move |this, cx| {
            if debounce {
                cx.background_executor().timer(REQUEST_DEBOUNCE).await;
            }
            let mut encoding = PositionEncoding::Utf16;
            let mut items = Vec::new();
            let mut incomplete = false;
            if let (Some((enc, client, context)), Some(mut params)) = (request, params) {
                encoding = enc;
                params["context"] = context;
                if let Ok(value) = client.request("textDocument/completion", params).await {
                    (items, incomplete) = parse_completion(&value);
                }
            }
            this.update(cx, |this, cx| {
                this.show_completions(anchor, items, incomplete, words, encoding, manual, cx)
            })
            .ok();
        }));
    }

    #[allow(clippy::too_many_arguments)]
    fn show_completions(
        &mut self,
        anchor: usize,
        items: Vec<CompletionItem>,
        incomplete: bool,
        words: Vec<String>,
        encoding: PositionEncoding,
        manual: bool,
        cx: &mut Context<Self>,
    ) {
        let head = self.state.primary().head;
        // The caret left the word while we waited.
        if head < anchor || word_start(self.state.buffer(), head) != anchor {
            return;
        }
        let mut entries: Vec<CompletionEntry> =
            items.into_iter().map(CompletionEntry::from_item).collect();
        if entries.is_empty() {
            entries.extend(words.into_iter().map(CompletionEntry::word));
        }
        let mut menu = CompletionMenu {
            anchor,
            entries: Arc::new(entries),
            matches: Vec::new(),
            selected: 0,
            scroll_top: 0,
            incomplete,
            encoding,
        };
        menu.filter(&self.state.buffer().slice(anchor..head));
        if menu.matches.is_empty() {
            self.completion = None;
            if manual {
                cx.emit(EditorEvent::Notice("No suggestions".to_string()));
            }
        } else {
            self.lsp.hover = None;
            self.completion = Some(menu);
        }
        cx.notify();
    }

    /// The word grew or shrank: filter again (asking again if the server
    /// said its list was incomplete).
    pub(super) fn refilter_completion(&mut self, cx: &mut Context<Self>) {
        let Some(menu) = self.completion.as_mut() else {
            return;
        };
        let primary = self.state.primary();
        let buffer = self.state.buffer();
        let head = primary.head;
        if !primary.is_empty() || head < menu.anchor || word_start(buffer, head) != menu.anchor {
            self.completion = None;
            cx.notify();
            return;
        }
        let query = buffer.slice(menu.anchor..head);
        let incomplete = menu.incomplete;
        menu.filter(&query);
        if incomplete {
            self.trigger_completion(false, None, cx);
        } else if menu.matches.is_empty() {
            self.completion = None;
        }
        cx.notify();
    }

    /// Inserts the selected item, first asking the server to fill it in
    /// (auto-imports in TypeScript) if it supports that.
    pub(super) fn accept_completion(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.completion.take() else {
            return false;
        };
        self.completion_task = None;
        let Some(entry) = menu.selected_entry().cloned() else {
            return false;
        };
        let resolver = self.lsp.doc.as_ref().map(|d| d.client.clone()).filter(|c| {
            c.capabilities()
                .is_some_and(|caps| caps["completionProvider"]["resolveProvider"] == true)
        });
        let (Some(client), Some(item)) = (resolver, entry.item.clone()) else {
            self.insert_completion(&entry, menu.anchor, menu.encoding, cx);
            return true;
        };
        let version = self.state.buffer().version();
        let (anchor, encoding) = (menu.anchor, menu.encoding);
        let request = client.request("completionItem/resolve", item.raw.clone());
        self.completion_task = Some(cx.spawn(async move |this, cx| {
            let timeout = cx.background_executor().timer(RESOLVE_TIMEOUT);
            let resolved = match futures::future::select(Box::pin(request), timeout).await {
                futures::future::Either::Left((Ok(value), _)) => Some(value),
                _ => None,
            };
            this.update(cx, |this, cx| {
                if this.state.buffer().version() != version {
                    return;
                }
                let entry = match resolved {
                    Some(value) => {
                        let mut item = *item;
                        item.apply_resolved(&value);
                        CompletionEntry::from_item(item)
                    }
                    None => entry,
                };
                this.insert_completion(&entry, anchor, encoding, cx);
            })
            .ok();
        }));
        true
    }

    fn insert_completion(
        &mut self,
        entry: &CompletionEntry,
        anchor: usize,
        encoding: PositionEncoding,
        cx: &mut Context<Self>,
    ) {
        let menu_anchor = anchor;
        let buffer = self.state.buffer();
        let head = self.state.primary().head;
        let (mut start, mut end) = (menu_anchor, head);
        if let Some(range) = entry.edit_range {
            let r = range_to_offsets(buffer, range, encoding);
            if r.start <= head {
                start = r.start;
            }
            end = r.end.max(head);
        }
        // Auto-imports and similar edits elsewhere, applied first.
        let mut additional: Vec<(Range<usize>, String)> = entry
            .additional
            .iter()
            .map(|e| {
                (
                    range_to_offsets(buffer, e.range, encoding),
                    e.new_text.clone(),
                )
            })
            .filter(|(r, _)| r.end <= start || r.start >= end)
            .collect();
        additional.sort_by_key(|(r, _)| r.start);
        if !additional.is_empty() {
            let shift: isize = additional
                .iter()
                .filter(|(r, _)| r.end <= start)
                .map(|(r, text)| text.len() as isize - r.len() as isize)
                .sum();
            self.state.replace_ranges(additional);
            start = (start as isize + shift) as usize;
            end = (end as isize + shift) as usize;
        }
        if entry.snippet {
            self.state
                .insert_snippet(start..end, &normalize_lsp_snippet(&entry.insert));
        } else {
            self.state
                .replace_and_select(start..end, &entry.insert, None, EditKind::Other);
        }
        self.request_autoscroll();
        self.after_edit(cx);
        cx.emit(EditorEvent::SelectionsChanged);
    }

    /// Accepting by mouse.
    pub(super) fn accept_completion_at(&mut self, row: usize, cx: &mut Context<Self>) {
        if let Some(menu) = self.completion.as_mut()
            && row < menu.matches.len()
        {
            menu.selected = row;
            self.accept_completion(cx);
        }
    }

    /// Keys while the menu is open. Returns true if the command was used.
    pub(super) fn completion_command(&mut self, name: &str, cx: &mut Context<Self>) -> bool {
        let Some(menu) = self.completion.as_mut() else {
            return false;
        };
        match name {
            "editor:move-up" => menu.select(-1),
            "editor:move-down" => menu.select(1),
            "editor:page-up" => menu.select(-(MENU_ROWS as isize)),
            "editor:page-down" => menu.select(MENU_ROWS as isize),
            "editor:newline" | "editor:tab" => return self.accept_completion(cx),
            "editor:cancel" => {
                self.completion = None;
                self.completion_task = None;
            }
            // Editing commands that keep the menu: filtered afterwards.
            "editor:backspace" | "editor:delete-word-left" | "editor:show-completions" => {
                return false;
            }
            _ => {
                self.completion = None;
                self.completion_task = None;
                return false;
            }
        }
        cx.notify();
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn menu(labels: &[&str]) -> CompletionMenu {
        let mut menu = CompletionMenu {
            anchor: 0,
            entries: Arc::new(
                labels
                    .iter()
                    .map(|l| CompletionEntry::word(l.to_string()))
                    .collect(),
            ),
            matches: Vec::new(),
            selected: 0,
            scroll_top: 0,
            incomplete: false,
            encoding: PositionEncoding::Utf16,
        };
        menu.filter("");
        menu
    }

    #[test]
    fn selection_wraps_and_scrolls() {
        let labels: Vec<String> = (0..15).map(|i| format!("w{i:02}")).collect();
        let refs: Vec<&str> = labels.iter().map(String::as_str).collect();
        let mut m = menu(&refs);
        m.select(-1);
        assert_eq!(m.selected, 14);
        assert_eq!(m.scroll_top, 5);
        m.select(1);
        assert_eq!((m.selected, m.scroll_top), (0, 0));
        m.select(10);
        assert_eq!((m.selected, m.scroll_top), (10, 1));
        m.filter("w1");
        assert_eq!(m.selected_entry().expect("test value").label, "w10");
    }
}

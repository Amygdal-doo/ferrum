//! Search Everywhere (Shift twice) and Find in files (⌘⇧F) in the shared
//! search modal: files, commands, settings and text across the project.

use std::path::PathBuf;
use std::sync::Arc;

use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    SearchCategory, SearchEvent, SearchItem, SearchModal, SearchMode, TextHit, TextMatcher,
    TextSearcher,
};
use ferrumeditor_core::find::SearchQuery;
use ferrumeditor_core::keymap::display_keys;
use ferrumeditor_core::project_search::{SearchLimits, search_project_where};

use super::Workspace;
use crate::settings::Settings;

const MAC: bool = cfg!(target_os = "macos");
/// Prefix of command ids, so they are told from file paths.
const COMMAND: &str = "command:";

impl Workspace {
    /// Opens the search modal (or closes it when it is already open in the
    /// same mode).
    pub(super) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(search) = self.search.take()
            && search.read(cx).mode() == mode
        {
            self.focus_active(window, cx);
            cx.notify();
            return;
        }
        if self.root.is_some() && self.file_list.is_none() {
            self.load_file_list(cx);
        }
        let mut items = Vec::new();
        if mode == SearchMode::Everywhere {
            items = self.search_file_items();
            items.extend(self.search_command_items(cx));
        }
        let searcher = self.root.clone().map(project_searcher);
        if mode == SearchMode::FindInFiles && searcher.is_none() {
            self.info("Open a folder first (⌘⇧O)", cx);
            return;
        }
        let prefill = self
            .active_editor()
            .and_then(|e| e.read(cx).selected_text())
            .filter(|t| !t.contains('\n'));
        let search = cx.new(|cx| SearchModal::new(mode, "Files", items, searcher, window, cx));
        if let Some(text) = prefill {
            search.update(cx, |s, cx| s.set_query(&text, window, cx));
        }
        let subscription = cx.subscribe_in(&search, window, Self::on_search_event);
        self._subscriptions.push(subscription);
        self.search = Some(search);
        cx.notify();
    }

    /// Hands a finished file walk to an open Search Everywhere.
    pub(super) fn refresh_search_files(&mut self, cx: &mut Context<Self>) {
        let Some(search) = self.search.clone() else {
            return;
        };
        if search.read(cx).mode() != SearchMode::Everywhere {
            return;
        }
        let mut items = self.search_file_items();
        items.extend(self.search_command_items(cx));
        search.update(cx, |s, cx| s.set_items(items, cx));
    }

    fn search_file_items(&self) -> Vec<SearchItem> {
        let (Some(root), Some(files)) = (self.root.clone(), self.file_list.clone()) else {
            return Vec::new();
        };
        files
            .iter()
            .map(|rel| {
                let label = rel.to_string_lossy().to_string();
                SearchItem::new(
                    SearchCategory::Items,
                    label,
                    root.join(rel).to_string_lossy().to_string(),
                )
                .icon(IconName::File)
            })
            .collect()
    }

    fn search_command_items(&self, cx: &App) -> Vec<SearchItem> {
        let settings = cx.global::<Settings>();
        settings
            .commands
            .iter()
            .filter(|c| {
                !c.name.starts_with("editor:move-") && !c.name.starts_with("editor:select-to")
            })
            .map(|c| {
                let mut item = SearchItem::new(
                    SearchCategory::Actions,
                    c.title(),
                    format!("{COMMAND}{}", c.name),
                );
                if let Some(keys) = settings.keymap.keys_for(&c.name).first() {
                    item = item.keys(display_keys(keys, MAC));
                }
                item
            })
            .collect()
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search = None;
        self.focus_active(window, cx);
        cx.notify();
        match event {
            SearchEvent::Open(item) => match item.id.strip_prefix(COMMAND) {
                Some(command) => self.run_command(command, window, cx),
                None => {
                    let path = PathBuf::from(item.id.as_ref());
                    self.open_file_at(&path, None, None, window, cx);
                }
            },
            SearchEvent::OpenHit(hit) => {
                let path = PathBuf::from(hit.source.as_ref());
                self.open_file_at(&path, Some(hit.line), Some(hit.column), window, cx);
            }
            SearchEvent::Dismissed => {}
        }
    }
}

/// Text search over the project's files (respecting `.gitignore`).
fn project_searcher(root: PathBuf) -> TextSearcher {
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let search = SearchQuery {
            pattern: query.text.clone(),
            case_sensitive: query.case_sensitive,
            whole_word: query.whole_word,
            regex: query.regex,
        };
        let limits = SearchLimits {
            max_results: limit,
            ..SearchLimits::default()
        };
        let allow = |rel: &std::path::Path| matcher.allows(&rel.to_string_lossy());
        let result = search_project_where(&root, &search, &limits, cancel, &allow)
            .map_err(|e| anyhow::anyhow!("{e}"))?;
        let mut hits = Vec::new();
        for file in result.files {
            let label = file
                .path
                .strip_prefix(&root)
                .unwrap_or(&file.path)
                .to_string_lossy()
                .to_string();
            let source = file.path.to_string_lossy().to_string();
            for m in file.matches {
                hits.push(TextHit {
                    source: source.clone().into(),
                    source_label: label.clone().into(),
                    line: m.line,
                    column: m.byte_range.start + m.line_offset,
                    preview: m.line_text.into(),
                    range: m.byte_range,
                });
            }
        }
        Ok(hits)
    })
}

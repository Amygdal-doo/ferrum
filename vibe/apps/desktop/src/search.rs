//! The shared search modal: Shift twice (Search Everywhere: the project's
//! files and chat threads, settings, menu commands, text), ⌘P (go to a
//! file or thread), ⌘⇧F (text in the project's files and its threads) and
//! ⌘F (find in the Editor column). The Search page stays in the sidebar.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use ferrum_ui::prelude::*;
use ferrum_ui::search::{
    DoubleShift, FindInFiles, FindInView, GoToItem, SearchCategory, SearchEvent, SearchEverywhere,
    SearchItem, SearchModal, SearchMode, TextHit, TextMatcher, TextSearcher,
};
use ferrumagent_ui::core::store::ThreadStore;
use ferrumeditor_core::find::SearchQuery;
use ferrumeditor_core::project_search::{SearchLimits, search_project_where};
use gpui::{KeyDownEvent, ModifiersChangedEvent};

use crate::app::FerrumEngineer;
use crate::layout::Column;
use crate::threads::ThreadIndex;

/// Files listed at most (the walk respects `.gitignore`).
const FILE_LIMIT: usize = 100_000;
const FILE: &str = "file:";
const THREAD: &str = "thread:";
/// Separates a thread's project from its id in an item id.
const SEP: char = '\u{1f}';

#[derive(Default)]
pub(crate) struct SearchState {
    pub modal: Option<Entity<SearchModal>>,
    pub double_shift: DoubleShift,
    /// The project's files (relative), once walked.
    files: Option<(PathBuf, Arc<Vec<PathBuf>>)>,
    files_task: Option<Task<()>>,
}

/// The project's files and every known chat thread, as Items.
pub(crate) fn search_items(
    root: Option<&Path>,
    files: &[PathBuf],
    index: &ThreadIndex,
) -> Vec<SearchItem> {
    let mut items = Vec::new();
    if let Some(root) = root {
        items.extend(files.iter().map(|rel| {
            SearchItem::new(
                SearchCategory::Items,
                rel.to_string_lossy().to_string(),
                format!("{FILE}{}", root.join(rel).display()),
            )
            .icon(IconName::File)
        }));
    }
    items.extend(
        index
            .all()
            .into_iter()
            .filter(|e| !e.thread.title.is_empty())
            .map(|e| {
                let project = e
                    .project
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_default();
                SearchItem::new(
                    SearchCategory::Items,
                    e.thread.title.clone(),
                    format!("{THREAD}{}{SEP}{}", e.project.display(), e.thread.id),
                )
                .detail(format!("Thread · {project}"))
                .icon(IconName::Sparkle)
            }),
    );
    items
}

/// Text in the project's files and in the current project's threads.
pub(crate) fn searcher(
    root: Option<PathBuf>,
    home: PathBuf,
    threads: Vec<(String, String)>,
) -> TextSearcher {
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        if let Some(root) = &root {
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
            let allow = |rel: &Path| matcher.allows(&rel.to_string_lossy());
            let result = search_project_where(root, &search, &limits, cancel, &allow)
                .map_err(|e| anyhow::anyhow!("{e}"))?;
            for file in result.files {
                let label = file
                    .path
                    .strip_prefix(root)
                    .unwrap_or(&file.path)
                    .to_string_lossy()
                    .to_string();
                let source = format!("{FILE}{}", file.path.display());
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
        }
        // Threads are not files: the mask leaves them out.
        if query.mask.trim().is_empty()
            && let Some(root) = &root
        {
            let store = ThreadStore::new(&home, root);
            for (id, title) in &threads {
                if hits.len() >= limit || cancel.load(Ordering::Relaxed) {
                    break;
                }
                let Ok(thread) = store.load(id) else { continue };
                let text: String = thread
                    .messages
                    .iter()
                    .map(|m| m.text())
                    .collect::<Vec<_>>()
                    .join("\n");
                let source = format!("{THREAD}{}{SEP}{id}", root.display());
                if !matcher.find_lines(&text, &source, title, limit, &mut hits) {
                    break;
                }
            }
        }
        Ok(hits)
    })
}

impl FerrumEngineer {
    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(modal) = self.search.modal.take()
            && modal.read(cx).mode() == mode
        {
            window.focus(&self.focus_handle().clone(), cx);
            cx.notify();
            return;
        }
        self.load_search_files(cx);
        let items = if mode == SearchMode::FindInFiles {
            Vec::new()
        } else {
            self.current_search_items()
        };
        let threads = self
            .project
            .as_deref()
            .map(|p| {
                self.index
                    .of(p)
                    .iter()
                    .map(|t| (t.id.clone(), t.title.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let searcher = searcher(self.project.clone(), self.agent_home.clone(), threads);
        let modal = cx.new(|cx| {
            SearchModal::new(mode, "Files and threads", items, Some(searcher), window, cx)
        });
        cx.subscribe_in(&modal, window, Self::on_search_event)
            .detach();
        self.search.modal = Some(modal);
        cx.notify();
    }

    fn current_search_items(&self) -> Vec<SearchItem> {
        let files = self
            .search
            .files
            .as_ref()
            .filter(|(root, _)| Some(root.as_path()) == self.project.as_deref())
            .map(|(_, files)| files.clone())
            .unwrap_or_default();
        search_items(self.project.as_deref(), &files, &self.index)
    }

    /// Walks the project once per project; an open modal gets the list
    /// when it arrives.
    fn load_search_files(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.project.clone() else {
            return;
        };
        if self.search.files.as_ref().is_some_and(|(r, _)| *r == root) {
            return;
        }
        self.search.files_task = Some(cx.spawn(async move |this, cx| {
            let walk_root = root.clone();
            let files = cx
                .background_spawn(async move {
                    ferrumeditor_core::file_tree::list_project_files(&walk_root, FILE_LIMIT)
                })
                .await;
            this.update(cx, |this, cx| {
                this.search.files = Some((root, Arc::new(files)));
                if let Some(modal) = this.search.modal.clone()
                    && modal.read(cx).mode() != SearchMode::FindInFiles
                {
                    let items = this.current_search_items();
                    modal.update(cx, |m, cx| m.set_items(items, cx));
                }
            })
            .ok();
        }));
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.modal = None;
        window.focus(&self.focus_handle().clone(), cx);
        cx.notify();
        let (id, line) = match event {
            SearchEvent::Open(item) => (item.id.to_string(), None),
            SearchEvent::OpenHit(hit) => (hit.source.to_string(), Some(hit.line as u32 + 1)),
            SearchEvent::Dismissed => return,
        };
        if let Some(path) = id.strip_prefix(FILE) {
            self.open_file(Path::new(path), line, window, cx);
        } else if let Some((project, thread)) = id
            .strip_prefix(THREAD)
            .and_then(|rest| rest.split_once(SEP))
        {
            self.open_thread(Path::new(project), thread, window, cx);
        }
    }

    /// ⌘F outside a pane with its own find: the Editor column's file.
    fn find_in_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = self
            .editor
            .clone()
            .filter(|_| self.layout.is_visible(Column::Editor))
            .and_then(|e| e.read(cx).active_editor());
        match editor {
            Some(editor) => editor.update(cx, |e, cx| e.open_find(window, cx)),
            None => cx.propagate(),
        }
    }

    /// Search listeners on the root element.
    pub(crate) fn search_listeners(
        &self,
        root: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        root.on_modifiers_changed(cx.listener(|this, e: &ModifiersChangedEvent, window, cx| {
            if this.search.double_shift.modifiers_changed(&e.modifiers) {
                this.toggle_search(SearchMode::Everywhere, window, cx);
            }
        }))
        .capture_key_down(cx.listener(|this, _: &KeyDownEvent, _, _| {
            this.search.double_shift.key_down();
        }))
        .on_action(cx.listener(|this, _: &SearchEverywhere, window, cx| {
            this.toggle_search(SearchMode::Everywhere, window, cx)
        }))
        .on_action(cx.listener(|this, _: &GoToItem, window, cx| {
            this.toggle_search(SearchMode::GoTo, window, cx)
        }))
        .on_action(cx.listener(|this, _: &FindInFiles, window, cx| {
            this.toggle_search(SearchMode::FindInFiles, window, cx)
        }))
        .on_action(cx.listener(|this, _: &FindInView, window, cx| this.find_in_view(window, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_ui::search::TextQuery;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn items_are_files_under_the_project() {
        let items = search_items(
            Some(Path::new("/p")),
            &[PathBuf::from("src/main.rs")],
            &ThreadIndex::default(),
        );
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].title.as_ref(), "src/main.rs");
        assert_eq!(items[0].id.as_ref(), "file:/p/src/main.rs");
        assert!(search_items(None, &[PathBuf::from("a")], &ThreadIndex::default()).is_empty());
    }

    #[test]
    fn searcher_reads_project_files_with_a_mask() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "x\nneedle\n").expect("write");
        std::fs::write(dir.path().join("b.md"), "needle\n").expect("write");
        let home = tempfile::tempdir().expect("home");
        let search = searcher(
            Some(dir.path().to_path_buf()),
            home.path().to_path_buf(),
            Vec::new(),
        );
        let query = TextQuery {
            text: "needle".into(),
            mask: "*.rs".into(),
            ..TextQuery::default()
        };
        let hits = search(&query, &AtomicBool::new(false), 50).expect("search");
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].source_label.as_ref(), "a.rs");
        assert_eq!(hits[0].line, 1);
    }
}

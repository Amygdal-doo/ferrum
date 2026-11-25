//! The shared search modal in the stand-alone FerrumGIT window: Shift
//! twice (Search Everywhere), ⌘⇧F (text in commit messages and changed
//! files) and ⌘F (the history search box). ⌘P stays the repository picker.
//! Embedded in a host (FerrumIDE, FerrumEngineer), the host's search runs.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use ferrum_ui::components::IconName;
use ferrum_ui::search::{
    DoubleShift, FindInFiles, FindInView, GoToItem, SearchCategory, SearchEvent, SearchEverywhere,
    SearchItem, SearchModal, SearchMode, TextHit, TextMatcher, TextSearcher,
};
use ferrumgit_domain::{Oid, RefKind, RepoId};
use gpui::{AppContext as _, Context, Entity, KeyDownEvent, ModifiersChangedEvent, Window};

use crate::app::GitView;
use crate::state::{AppState, Page};

/// Changed files larger than this are not searched.
const MAX_FILE_BYTES: u64 = 2 * 1024 * 1024;

const REPO: &str = "repo:";
const FILE: &str = "file:";
const COMMIT: &str = "commit:";

/// Search state of a [`GitView`].
#[derive(Default)]
pub(crate) struct Search {
    pub modal: Option<Entity<SearchModal>>,
    pub double_shift: DoubleShift,
}

/// Repositories and the open repository's changed files (Items); its
/// branches, tags and loaded commits (Symbols).
pub(crate) fn search_items(state: &AppState) -> Vec<SearchItem> {
    let mut items: Vec<SearchItem> = state
        .repos
        .iter()
        .map(|r| {
            SearchItem::new(
                SearchCategory::Items,
                r.name.clone(),
                format!("{REPO}{}", r.id),
            )
            .detail(r.path.clone())
            .icon(IconName::Folder)
        })
        .collect();
    let Some(data) = state
        .active
        .and_then(|id| state.views.get(&id)?.data.clone())
    else {
        return items;
    };
    let snapshot = &data.snapshot;
    items.extend(snapshot.status.files.iter().map(|f| {
        SearchItem::new(
            SearchCategory::Items,
            f.path.clone(),
            format!("{FILE}{}", f.path),
        )
        .detail("Changed")
        .icon(IconName::File)
    }));
    items.extend(snapshot.refs.iter().map(|r| {
        let (detail, icon) = match r.kind {
            Some(RefKind::Tag) => ("Tag", IconName::Bookmark),
            Some(RefKind::RemoteBranch) => ("Remote branch", IconName::GitBranch),
            _ => ("Branch", IconName::GitBranch),
        };
        SearchItem::new(
            SearchCategory::Symbols,
            r.name.clone(),
            format!("{COMMIT}{}", r.target.0),
        )
        .detail(detail)
        .icon(icon)
    }));
    items.extend(snapshot.commits.iter().map(|c| {
        SearchItem::new(
            SearchCategory::Symbols,
            c.summary.clone(),
            format!("{COMMIT}{}", c.oid.0),
        )
        .detail(format!("{} · {}", c.oid.short(), c.author_name))
        .icon(IconName::Circle)
    }));
    items
}

/// What the text search reads: loaded commit messages and the changed
/// files of the open repository.
#[derive(Default)]
pub(crate) struct TextSources {
    /// `(oid, summary)`.
    pub commits: Vec<(String, String)>,
    pub root: Option<PathBuf>,
    pub files: Vec<String>,
}

pub(crate) fn text_sources(state: &AppState) -> TextSources {
    let Some(id) = state.active else {
        return TextSources::default();
    };
    let Some(data) = state.views.get(&id).and_then(|v| v.data.clone()) else {
        return TextSources::default();
    };
    TextSources {
        commits: data
            .snapshot
            .commits
            .iter()
            .map(|c| (c.oid.0.clone(), c.summary.clone()))
            .collect(),
        root: state.entry(id).map(|e| PathBuf::from(e.path)),
        files: data
            .snapshot
            .status
            .files
            .iter()
            .map(|f| f.path.clone())
            .collect(),
    }
}

pub(crate) fn searcher(sources: TextSources) -> TextSearcher {
    let sources = Arc::new(sources);
    Arc::new(move |query, cancel, limit| {
        let matcher = TextMatcher::new(query)?;
        let mut hits = Vec::new();
        if let Some(root) = &sources.root {
            for path in &sources.files {
                if cancel.load(std::sync::atomic::Ordering::Relaxed) {
                    return Ok(hits);
                }
                if !matcher.allows(path) {
                    continue;
                }
                let Some(text) = read_text(&root.join(path)) else {
                    continue;
                };
                if !matcher.find_lines(&text, &format!("{FILE}{path}"), path, limit, &mut hits) {
                    return Ok(hits);
                }
            }
        }
        for (oid, summary) in &sources.commits {
            let short = oid.get(..7).unwrap_or(oid);
            if !matcher.find_lines(summary, &format!("{COMMIT}{oid}"), short, limit, &mut hits) {
                break;
            }
        }
        Ok(hits)
    })
}

fn read_text(path: &Path) -> Option<String> {
    if std::fs::metadata(path).ok()?.len() > MAX_FILE_BYTES {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    if bytes.iter().take(8192).any(|b| *b == 0) {
        return None;
    }
    String::from_utf8(bytes).ok()
}

impl GitView {
    /// Only the stand-alone window searches; a host has its own search.
    fn searches(&self) -> bool {
        self.options.show_repo_tabs
    }

    pub(crate) fn toggle_search(
        &mut self,
        mode: SearchMode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(modal) = self.search.modal.take()
            && modal.read(cx).mode() == mode
        {
            self.focus_root(window, cx);
            cx.notify();
            return;
        }
        let items = if mode == SearchMode::Everywhere {
            search_items(&self.state)
        } else {
            Vec::new()
        };
        let searcher = searcher(text_sources(&self.state));
        let modal =
            cx.new(|cx| SearchModal::new(mode, "Repositories", items, Some(searcher), window, cx));
        cx.subscribe_in(&modal, window, Self::on_search_event)
            .detach();
        self.search.modal = Some(modal);
        cx.notify();
    }

    fn on_search_event(
        &mut self,
        _: &Entity<SearchModal>,
        event: &SearchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search.modal = None;
        self.focus_root(window, cx);
        cx.notify();
        let id = match event {
            SearchEvent::Open(item) => item.id.to_string(),
            SearchEvent::OpenHit(hit) => hit.source.to_string(),
            SearchEvent::Dismissed => return,
        };
        let line = match event {
            SearchEvent::OpenHit(TextHit { line, .. }) => Some(*line as u32 + 1),
            _ => None,
        };
        if let Some(repo) = id.strip_prefix(REPO) {
            if let Some(entry) = self.state.repos.iter().find(|r| r.id.to_string() == repo) {
                self.open_path(PathBuf::from(&entry.path), cx);
            }
        } else if let Some(oid) = id.strip_prefix(COMMIT) {
            if let Some(repo) = self.state.active {
                self.reveal_commit(repo, &Oid(oid.to_owned()), cx);
            }
        } else if let Some(path) = id.strip_prefix(FILE)
            && let Some(repo) = self.state.active
        {
            self.edit_file(repo, path, line, window, cx);
        }
    }

    fn active_repo(&self) -> Option<RepoId> {
        self.state.active
    }

    /// ⌘F: the history search box (message, author, SHA).
    fn find_in_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.active_repo() else {
            cx.propagate();
            return;
        };
        if self.state.view(id).page != Page::History {
            self.set_page(id, Page::History, cx);
        }
        self.inputs
            .search
            .update(cx, |input, cx| input.focus(window, cx));
    }

    /// Search listeners on the root element.
    pub(crate) fn search_listeners(
        &self,
        root: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        use gpui::InteractiveElement as _;
        let root = root.on_action(
            cx.listener(|this, _: &FindInView, window, cx| this.find_in_view(window, cx)),
        );
        if !self.searches() {
            return root;
        }
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
        .on_action(cx.listener(|this, _: &FindInFiles, window, cx| {
            this.toggle_search(SearchMode::FindInFiles, window, cx)
        }))
        // ⌘P is bound to the repository picker in FerrumGIT's own context;
        // a GoToItem from elsewhere (the menu) opens it too.
        .on_action(cx.listener(|this, _: &GoToItem, window, cx| {
            this.toggle_search(SearchMode::GoTo, window, cx)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrum_ui::search::TextQuery;
    use std::sync::atomic::AtomicBool;

    fn query(text: &str) -> TextQuery {
        TextQuery {
            text: text.into(),
            ..TextQuery::default()
        }
    }

    #[test]
    fn items_list_repositories_without_an_open_one() {
        let mut state = AppState::default();
        let entry = ferrumgit_domain::RepoEntry::new("/tmp/ferrum".to_owned());
        state.repos.push(entry.clone());
        let items = search_items(&state);
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].category, SearchCategory::Items);
        assert_eq!(items[0].id.as_ref(), format!("{REPO}{}", entry.id));
    }

    #[test]
    fn searcher_finds_commit_messages_and_changed_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(dir.path().join("a.rs"), "fn main() {}\nlet needle = 1;\n").expect("write");
        std::fs::write(dir.path().join("b.md"), "needle\n").expect("write");
        let search = searcher(TextSources {
            commits: vec![("0123456789".into(), "Fix the needle bug".into())],
            root: Some(dir.path().to_path_buf()),
            files: vec!["a.rs".into(), "b.md".into(), "gone.rs".into()],
        });
        let hits = search(&query("needle"), &AtomicBool::new(false), 100).expect("search");
        let sources: Vec<_> = hits.iter().map(|h| h.source.to_string()).collect();
        assert_eq!(sources, ["file:a.rs", "file:b.md", "commit:0123456789"]);
        assert_eq!(hits[0].line, 1);
        assert_eq!(hits[2].source_label.as_ref(), "0123456");

        let masked = TextQuery {
            mask: "*.rs".into(),
            ..query("needle")
        };
        let hits = search(&masked, &AtomicBool::new(false), 100).expect("search");
        assert_eq!(hits.len(), 2);
        let hits = search(&query("needle"), &AtomicBool::new(false), 1).expect("search");
        assert_eq!(hits.len(), 1);
    }
}

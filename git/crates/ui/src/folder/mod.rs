//! A folder that is not a git work tree, in an embedded view (a terminal
//! tab in `~`, a project without git): instead of an error the pane shows
//! the files changed there since it started watching, diffed against a
//! snapshot taken then, and the repositories found below the folder
//! (picking one shows it as usual, with a way back).

mod diff;
mod scan;
#[cfg(test)]
mod tests;
mod view;
mod watch;

pub(crate) use view::{picked_strip, render};

use crate::app::GitView;
use crate::embed::GitViewEvent;
use crate::state::Page;
use diff::Change;
use ferrumgit_domain::FileDiff;
use gpui::{AppContext as _, Context, Task};
use scan::{Limits, Snapshot, TooBig};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

/// How often the watch loop looks at the change flag.
const TICK: Duration = Duration::from_millis(500);
/// Without a file watcher, rescan every this many ticks (3 s).
const POLL_TICKS: u32 = 6;

/// The folder pane's two pages: the host's "Code changes" and
/// "FerrumGIT" (history) toggles land on them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FolderPage {
    Changes,
    Repositories,
}

impl From<Page> for FolderPage {
    fn from(page: Page) -> Self {
        if page == Page::WorkingCopy {
            Self::Changes
        } else {
            Self::Repositories
        }
    }
}

/// The snapshot changes are measured against.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Baseline {
    /// Not taken yet: the Changes page was not shown.
    Pending,
    Taking,
    /// Taken at `since` (local time, for the status line).
    Ready {
        since: String,
        files: usize,
    },
    TooBig(TooBig),
}

pub(crate) struct FolderMode {
    pub path: PathBuf,
    pub page: FolderPage,
    pub baseline: Baseline,
    base: Option<Arc<Snapshot>>,
    /// The last scan (files unchanged since are not read again).
    current: Option<Arc<Snapshot>>,
    pub changes: Vec<Change>,
    /// Selected change (relative path) and its diff.
    pub selected: Option<String>,
    pub diff: Option<Arc<FileDiff>>,
    /// Repositories below; `None` while searching.
    pub repos: Option<Vec<PathBuf>>,
    /// The repository below being shown (picked from the list).
    pub picked: Option<PathBuf>,
    /// Bumped by every new baseline: older scans are dropped.
    generation: u64,
    /// Set by the watcher when a watched file changes.
    dirty: Arc<AtomicBool>,
    scanning: bool,
    /// No watcher could be started: rescan on a timer.
    polling: bool,
    ticks: u32,
    _watcher: Option<notify::RecommendedWatcher>,
    _watch: Option<Task<()>>,
}

impl FolderMode {
    fn new(path: PathBuf, page: FolderPage) -> Self {
        Self {
            path,
            page,
            baseline: Baseline::Pending,
            base: None,
            current: None,
            changes: Vec::new(),
            selected: None,
            diff: None,
            repos: None,
            picked: None,
            generation: 0,
            dirty: Arc::new(AtomicBool::new(false)),
            scanning: false,
            polling: false,
            ticks: 0,
            _watcher: None,
            _watch: None,
        }
    }
}

fn home() -> Option<PathBuf> {
    ferrum_ui::project::home_dir()
}

/// Whether `path` is inside a git work tree.
#[must_use]
pub fn in_work_tree(path: &Path) -> bool {
    ferrum_ui::project::git_root(path).is_some()
}

impl GitView {
    /// The folder pane is shown (a folder without git, nothing picked).
    pub(crate) fn in_folder(&self) -> bool {
        self.folder.is_some() && self.state.active.is_none()
    }

    /// Shows `path`, a folder outside any work tree, as a folder pane.
    pub(crate) fn enter_folder(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let page = self
            .pending_page
            .take()
            .map_or(FolderPage::Changes, FolderPage::from);
        self.close_repo(cx);
        self.folder = Some(FolderMode::new(path, page));
        self.find_folder_repos(cx);
        if page == FolderPage::Changes {
            self.take_baseline(cx);
        }
        cx.notify();
    }

    /// Closes the shown repository (one at a time here).
    fn close_repo(&mut self, cx: &mut Context<Self>) {
        for id in std::mem::take(&mut self.state.open) {
            self.state.views.remove(&id);
        }
        if self.state.active.is_some() {
            self.activate(None, cx);
        }
    }

    pub(crate) fn set_folder_page(&mut self, page: FolderPage, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        folder.page = page;
        if page == FolderPage::Changes && folder.baseline == Baseline::Pending {
            self.take_baseline(cx);
        }
        cx.notify();
    }

    /// Shows a repository from the list (on the page matching the folder's).
    pub(crate) fn pick_repo(&mut self, repo: PathBuf, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        folder.picked = Some(repo.clone());
        self.pending_page = Some(match folder.page {
            FolderPage::Changes => Page::WorkingCopy,
            FolderPage::Repositories => Page::History,
        });
        self.add_and_open(repo, cx);
    }

    /// From a picked repository back to the folder.
    pub(crate) fn back_to_folder(&mut self, cx: &mut Context<Self>) {
        if let Some(folder) = self.folder.as_mut() {
            folder.picked = None;
        }
        self.close_repo(cx);
        cx.notify();
    }

    fn find_folder_repos(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        folder.repos = None;
        let path = folder.path.clone();
        let task = cx.background_spawn(async move {
            scan::find_repos(&path, scan::REPO_DEPTH, home().as_deref())
        });
        cx.spawn(async move |this, cx| {
            let repos = task.await;
            this.update(cx, |app, cx| {
                if let Some(folder) = app.folder.as_mut().filter(|f| f.repos.is_none()) {
                    folder.repos = Some(repos);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Takes a new baseline ("Reset"): changes count from now.
    pub(crate) fn take_baseline(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        folder.generation += 1;
        folder.baseline = Baseline::Taking;
        folder.base = None;
        folder.current = None;
        folder.changes.clear();
        folder.selected = None;
        folder.diff = None;
        folder._watcher = None;
        folder._watch = None;
        let generation = folder.generation;
        let path = folder.path.clone();
        let task = cx.background_spawn(async move {
            scan::snapshot(&path, Limits::default(), home().as_deref(), None)
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |app, cx| app.baseline_taken(generation, result, cx))
                .ok();
        })
        .detach();
        cx.notify();
    }

    fn baseline_taken(
        &mut self,
        generation: u64,
        result: Result<Snapshot, TooBig>,
        cx: &mut Context<Self>,
    ) {
        let Some(folder) = self.folder.as_mut().filter(|f| f.generation == generation) else {
            return;
        };
        match result {
            Ok(snapshot) => {
                folder.baseline = Baseline::Ready {
                    since: chrono::Local::now().format("%H:%M").to_string(),
                    files: snapshot.files.len(),
                };
                let snapshot = Arc::new(snapshot);
                folder.base = Some(snapshot.clone());
                folder.current = Some(snapshot);
                self.watch_folder(cx);
            }
            Err(too_big) => folder.baseline = Baseline::TooBig(too_big),
        }
        cx.notify();
    }

    /// Selects a changed file and diffs it against the baseline.
    pub(crate) fn select_folder_file(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        let text = |snapshot: &Option<Arc<Snapshot>>| {
            snapshot
                .as_ref()
                .and_then(|s| s.files.get(&path))
                .map(|e| e.text.clone())
        };
        let (old, new) = (text(&folder.base), text(&folder.current));
        folder.diff = Some(Arc::new(diff::file_diff(
            &path,
            old.as_deref(),
            new.as_deref(),
        )));
        folder.selected = Some(path);
        cx.notify();
    }

    pub(crate) fn close_folder_diff(&mut self, cx: &mut Context<Self>) {
        if let Some(folder) = self.folder.as_mut() {
            folder.selected = None;
            folder.diff = None;
        }
        cx.notify();
    }

    /// Asks the host to open a file of the folder, like the diff's "Open".
    pub(crate) fn open_folder_file(
        &mut self,
        path: &str,
        line: Option<u32>,
        cx: &mut Context<Self>,
    ) {
        if let Some(folder) = &self.folder {
            cx.emit(GitViewEvent::OpenFile {
                path: folder.path.join(path),
                line,
            });
        }
    }

    /// ⌘R in the folder pane: rescan now, search for repositories again.
    pub(crate) fn reload_folder(&mut self, cx: &mut Context<Self>) {
        if let Some(folder) = &self.folder {
            folder.dirty.store(true, Ordering::Relaxed);
        }
        self.find_folder_repos(cx);
    }
}

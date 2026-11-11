//! The tree view: the opened folder on the left, with expand / collapse,
//! keyboard navigation and a context menu. File operations are emitted as
//! [`TreeEvent`]s for the workspace to carry out (dialogs, Trash).

mod render;

use std::path::{Path, PathBuf};

use ferrumeditor_core::file_tree::{FileTree, VisibleEntry};
use gpui::{
    App, Context, EventEmitter, FocusHandle, Focusable, Pixels, Point, ScrollStrategy,
    UniformListScrollHandle, Window,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TreeEvent {
    Open(PathBuf),
    NewFile { dir: PathBuf },
    NewFolder { dir: PathBuf },
    Rename(PathBuf),
    Delete(PathBuf),
    RevealInFinder(PathBuf),
}

/// A right-click menu open at `position` for `path`.
#[derive(Debug, Clone)]
struct ContextMenuState {
    path: PathBuf,
    position: Point<Pixels>,
}

pub struct TreeView {
    tree: FileTree,
    /// Flattened visible entries, rebuilt after every tree change.
    entries: Vec<VisibleEntry>,
    selected: Option<PathBuf>,
    active_file: Option<PathBuf>,
    focus_handle: FocusHandle,
    scroll: UniformListScrollHandle,
    menu: Option<ContextMenuState>,
}

impl EventEmitter<TreeEvent> for TreeView {}

impl Focusable for TreeView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl TreeView {
    pub fn new(root: PathBuf, _window: &mut Window, cx: &mut Context<Self>) -> Self {
        let tree = FileTree::new(root);
        let entries = tree.visible_entries();
        Self {
            tree,
            entries,
            selected: None,
            active_file: None,
            focus_handle: cx.focus_handle(),
            scroll: UniformListScrollHandle::new(),
            menu: None,
        }
    }

    pub fn root(&self) -> &Path {
        self.tree.root()
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.entries = self.tree.visible_entries();
        cx.notify();
    }

    /// Re-reads every loaded folder (after changes on disk).
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        self.tree.refresh();
        self.rebuild(cx);
    }

    /// Expands the ancestors of `path`, selects it and scrolls to it.
    pub fn reveal(&mut self, path: &Path, cx: &mut Context<Self>) -> bool {
        if !self.tree.reveal(path) {
            return false;
        }
        self.selected = Some(path.to_path_buf());
        self.rebuild(cx);
        self.scroll_to_selected();
        true
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    pub fn is_focused(&self, window: &Window) -> bool {
        self.focus_handle.is_focused(window)
    }

    pub fn collapse_all(&mut self, cx: &mut Context<Self>) {
        let expanded: Vec<PathBuf> = self
            .entries
            .iter()
            .filter(|e| e.is_dir && e.expanded)
            .map(|e| e.path.clone())
            .collect();
        for path in expanded.iter().rev() {
            self.tree.collapse(path);
        }
        self.rebuild(cx);
    }

    /// Marks the active editor's file (without moving the selection).
    pub fn set_active_file(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.active_file != path {
            self.active_file = path;
            cx.notify();
        }
    }

    fn selected_index(&self) -> Option<usize> {
        let selected = self.selected.as_ref()?;
        self.entries.iter().position(|e| &e.path == selected)
    }

    fn scroll_to_selected(&self) {
        if let Some(index) = self.selected_index() {
            self.scroll.scroll_to_item(index, ScrollStrategy::Center);
        }
    }

    fn select_index(&mut self, index: usize, cx: &mut Context<Self>) {
        if let Some(entry) = self.entries.get(index) {
            self.selected = Some(entry.path.clone());
            self.scroll_to_selected();
            cx.notify();
        }
    }

    /// Folder for new files: the selected folder, the selected file's
    /// parent, or the root.
    fn target_dir(&self) -> PathBuf {
        match self.selected.as_ref() {
            Some(path) if path.is_dir() => path.clone(),
            Some(path) => path
                .parent()
                .map_or_else(|| self.root().to_path_buf(), Path::to_path_buf),
            None => self.root().to_path_buf(),
        }
    }

    /// Click (or Enter) on an entry: folders toggle, files open.
    fn activate(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.selected = Some(path.clone());
        self.menu = None;
        if path.is_dir() {
            self.tree.toggle(&path);
            self.rebuild(cx);
        } else {
            cx.emit(TreeEvent::Open(path));
            cx.notify();
        }
    }

    fn emit_for_selected(
        &mut self,
        make: impl FnOnce(PathBuf) -> TreeEvent,
        cx: &mut Context<Self>,
    ) {
        if let Some(path) = self.selected.clone() {
            cx.emit(make(path));
        }
    }

    pub fn run_command(
        &mut self,
        name: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.menu = None;
        match name {
            "tree-view:select-next" => {
                let next = self
                    .selected_index()
                    .map_or(0, |i| (i + 1).min(self.entries.len().saturating_sub(1)));
                self.select_index(next, cx);
            }
            "tree-view:select-previous" => {
                let previous = self.selected_index().map_or(0, |i| i.saturating_sub(1));
                self.select_index(previous, cx);
            }
            "tree-view:expand" => {
                if let Some(index) = self.selected_index() {
                    let entry = self.entries[index].clone();
                    if entry.is_dir && !entry.expanded {
                        self.tree.expand(&entry.path);
                        self.rebuild(cx);
                    } else if entry.is_dir {
                        let child = self
                            .entries
                            .get(index + 1)
                            .filter(|e| e.depth > entry.depth);
                        if child.is_some() {
                            self.select_index(index + 1, cx);
                        }
                    }
                }
            }
            "tree-view:collapse" => {
                if let Some(index) = self.selected_index() {
                    let entry = self.entries[index].clone();
                    if entry.is_dir && entry.expanded {
                        self.tree.collapse(&entry.path);
                        self.rebuild(cx);
                    } else if let Some(parent) = self.entries[..index]
                        .iter()
                        .rposition(|e| e.depth < entry.depth)
                    {
                        self.select_index(parent, cx);
                    }
                }
            }
            "tree-view:open-selected" => {
                if let Some(path) = self.selected.clone() {
                    self.activate(path, cx);
                }
            }
            "tree-view:new-file" => cx.emit(TreeEvent::NewFile {
                dir: self.target_dir(),
            }),
            "tree-view:new-folder" => cx.emit(TreeEvent::NewFolder {
                dir: self.target_dir(),
            }),
            "tree-view:rename" => self.emit_for_selected(TreeEvent::Rename, cx),
            "tree-view:delete" => self.emit_for_selected(TreeEvent::Delete, cx),
            "tree-view:reveal-in-finder" => self.emit_for_selected(TreeEvent::RevealInFinder, cx),
            "tree-view:collapse-all" => self.collapse_all(cx),
            _ => return false,
        }
        true
    }
}

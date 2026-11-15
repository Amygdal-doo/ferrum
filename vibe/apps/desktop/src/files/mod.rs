//! The Files pane: the project's tree on the far left. Folders expand on
//! click (read when first opened, `.gitignore` respected), files open in
//! FerrumIDE on double-click or Enter, git status colours the names,
//! and rows drag onto the chat as context (⌘-click selects several).
//! The header row is drawn by the window (`crate::columns`).

mod git_marks;
mod tree;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use ferrum_ui::components::IconSize;
use ferrum_ui::gpui::{MouseButton, ScrollStrategy, UniformListScrollHandle};
use ferrum_ui::prelude::*;

pub use git_marks::{FileStatus, GitMarks};
pub use tree::{FileTree, Row};

actions!(
    vibe_files,
    [
        SelectPrevious,
        SelectNext,
        ExpandSelected,
        CollapseSelected,
        OpenSelected
    ]
);

pub const KEY_CONTEXT: &str = "VibeFiles";
const ROW_HEIGHT: f32 = 24.;
const INDENT: f32 = 12.;

pub fn bind_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("up", SelectPrevious, context),
        KeyBinding::new("down", SelectNext, context),
        KeyBinding::new("right", ExpandSelected, context),
        KeyBinding::new("left", CollapseSelected, context),
        KeyBinding::new("enter", OpenSelected, context),
    ]);
}

/// Files and folders dragged from the tree (the chat takes them as
/// context). Also the drag preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedPaths(pub Vec<PathBuf>);

impl Render for DraggedPaths {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let label = match self.0.as_slice() {
            [one] => one
                .file_name()
                .map_or_else(|| one.display().to_string(), |n| n.to_string_lossy().into()),
            many => format!("{} items", many.len()),
        };
        div()
            .px_2()
            .py_0p5()
            .rounded(px(6.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.elevated_surface_background)
            .text_size(px(12.))
            .text_color(colors.text)
            .child(label)
    }
}

/// `paths` without repeats and without paths inside another one of them
/// (a folder dropped with some of its files is the folder), in order.
#[must_use]
pub fn without_nested(paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for path in paths {
        let covered = paths
            .iter()
            .any(|other| other != path && path.starts_with(other));
        if !covered && !out.contains(path) {
            out.push(path.clone());
        }
    }
    out
}

pub enum FilesEvent {
    /// Open this file (in FerrumIDE).
    Open(PathBuf),
}

impl EventEmitter<FilesEvent> for FilesView {}

pub struct FilesView {
    tree: Option<FileTree>,
    /// Trees of the projects shown before, with their expanded folders.
    others: HashMap<PathBuf, FileTree>,
    rows: Vec<Row>,
    marks: GitMarks,
    /// Selected rows, the last one the cursor.
    selection: Vec<PathBuf>,
    scroll: UniformListScrollHandle,
    focus_handle: FocusHandle,
    marks_task: Option<Task<()>>,
    refresh_task: Option<Task<()>>,
}

impl FilesView {
    pub fn new(root: Option<PathBuf>, cx: &mut Context<Self>) -> Self {
        let mut view = Self {
            tree: None,
            others: HashMap::new(),
            rows: Vec::new(),
            marks: GitMarks::default(),
            selection: Vec::new(),
            scroll: UniformListScrollHandle::new(),
            focus_handle: cx.focus_handle(),
            marks_task: None,
            refresh_task: None,
        };
        if let Some(root) = root {
            view.set_root(root, cx);
        }
        view
    }

    #[must_use]
    pub fn root(&self) -> Option<&Path> {
        self.tree.as_ref().map(FileTree::root)
    }

    /// Shows `root`'s tree (as it was left, if it was shown before).
    pub fn set_root(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        if self.root() == Some(root.as_path()) {
            return;
        }
        if let Some(old) = self.tree.take() {
            self.others.insert(old.root().to_path_buf(), old);
        }
        let tree = match self.others.remove(&root) {
            Some(mut tree) => {
                tree.refresh();
                tree
            }
            None => FileTree::new(root),
        };
        self.tree = Some(tree);
        self.selection.clear();
        self.marks = GitMarks::default();
        self.rebuild(cx);
        self.reload_marks(cx);
    }

    /// Re-reads the folders shown and the git status.
    pub fn refresh(&mut self, cx: &mut Context<Self>) {
        if let Some(tree) = &mut self.tree {
            tree.refresh();
        }
        self.selection.retain(|p| p.exists());
        self.rebuild(cx);
        self.reload_marks(cx);
    }

    /// [`Self::refresh`] with the folders read off the UI thread (for the
    /// agent's frequent change reports).
    pub fn refresh_in_background(&mut self, cx: &mut Context<Self>) {
        let Some(tree) = &self.tree else {
            return;
        };
        let root = tree.root().to_path_buf();
        let dirs = tree.loaded_dirs();
        let read = cx.background_spawn({
            let root = root.clone();
            async move { FileTree::read_again(&root, dirs) }
        });
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            let reads = read.await;
            this.update(cx, |this, cx| {
                let Some(tree) = this.tree.as_mut().filter(|t| t.root() == root) else {
                    return;
                };
                tree.apply_reads(reads);
                this.selection.retain(|p| p.exists());
                this.rebuild(cx);
                this.reload_marks(cx);
            })
            .ok();
        }));
    }

    fn reload_marks(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root().map(Path::to_path_buf) else {
            return;
        };
        let read = cx.background_spawn({
            let root = root.clone();
            async move { GitMarks::read(&root) }
        });
        self.marks_task = Some(cx.spawn(async move |this, cx| {
            let marks = read.await;
            this.update(cx, |this, cx| {
                if this.root() == Some(root.as_path()) && this.marks != marks {
                    this.marks = marks;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    fn rebuild(&mut self, cx: &mut Context<Self>) {
        self.rows = self.tree.as_ref().map(FileTree::rows).unwrap_or_default();
        cx.notify();
    }

    #[cfg(test)]
    #[must_use]
    pub fn rows(&self) -> &[Row] {
        &self.rows
    }

    #[cfg(test)]
    #[must_use]
    pub fn selection(&self) -> &[PathBuf] {
        &self.selection
    }

    /// Expands or collapses folder `dir`.
    pub fn toggle(&mut self, dir: &Path, cx: &mut Context<Self>) {
        if let Some(tree) = &mut self.tree {
            tree.toggle(dir);
            self.rebuild(cx);
        }
    }

    pub fn collapse_all(&mut self, cx: &mut Context<Self>) {
        if let Some(tree) = &mut self.tree {
            tree.collapse_all();
            self.rebuild(cx);
        }
    }

    /// Selects `path`; with `add`, adds it to (or removes it from) the
    /// selection instead.
    pub fn select(&mut self, path: &Path, add: bool, cx: &mut Context<Self>) {
        if add {
            if let Some(i) = self.selection.iter().position(|p| p == path) {
                self.selection.remove(i);
            } else {
                self.selection.push(path.to_path_buf());
            }
        } else {
            self.selection = vec![path.to_path_buf()];
        }
        cx.notify();
    }

    /// Opens a file (event) or expands / collapses a folder.
    pub fn open(&mut self, path: &Path, is_dir: bool, cx: &mut Context<Self>) {
        if is_dir {
            self.toggle(path, cx);
        } else {
            cx.emit(FilesEvent::Open(path.to_path_buf()));
        }
    }

    /// What dragging `path` carries: the whole selection when `path` is in
    /// it, else `path` alone.
    #[must_use]
    pub fn dragged(&self, path: &Path) -> DraggedPaths {
        if self.selection.len() > 1 && self.selection.iter().any(|p| p == path) {
            let in_order: Vec<PathBuf> = self
                .rows
                .iter()
                .filter(|r| self.selection.contains(&r.path))
                .map(|r| r.path.clone())
                .collect();
            DraggedPaths(without_nested(&in_order))
        } else {
            DraggedPaths(vec![path.to_path_buf()])
        }
    }

    fn cursor(&self) -> Option<usize> {
        let path = self.selection.last()?;
        self.rows.iter().position(|r| &r.path == path)
    }

    fn move_cursor(&mut self, down: bool, cx: &mut Context<Self>) {
        if self.rows.is_empty() {
            return;
        }
        let next = match (self.cursor(), down) {
            (None, _) => 0,
            (Some(i), true) => (i + 1).min(self.rows.len() - 1),
            (Some(i), false) => i.saturating_sub(1),
        };
        self.selection = vec![self.rows[next].path.clone()];
        self.scroll.scroll_to_item(next, ScrollStrategy::Center);
        cx.notify();
    }

    fn selected_row(&self) -> Option<Row> {
        self.cursor().map(|i| self.rows[i].clone())
    }

    fn expand_selected(&mut self, expand: bool, cx: &mut Context<Self>) {
        let Some(row) = self.selected_row() else {
            return;
        };
        if row.is_dir && row.expanded != expand {
            self.toggle(&row.path, cx);
        } else if !expand && let Some(parent) = row.path.parent() {
            // ← on a file or a closed folder goes to its folder.
            if self.rows.iter().any(|r| r.path == parent) {
                self.selection = vec![parent.to_path_buf()];
                cx.notify();
            }
        }
    }

    fn open_selected(&mut self, cx: &mut Context<Self>) {
        if let Some(row) = self.selected_row() {
            self.open(&row.path, row.is_dir, cx);
        }
    }

    fn render_rows(
        &self,
        range: std::ops::Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors().clone();
        let (selected_bg, hover_bg) = (ferrum_ui::active_fill(cx), ferrum_ui::hover_fill(cx));
        let root = self.root().map(Path::to_path_buf).unwrap_or_default();
        range
            .filter_map(|i| self.rows.get(i).map(|row| (i, row)))
            .map(|(i, row)| {
                let selected = self.selection.contains(&row.path);
                let name_color = match self.marks.status(&row.path, row.is_dir) {
                    Some(FileStatus::Untracked | FileStatus::Added) => colors.version_control_added,
                    Some(FileStatus::Modified) => colors.version_control_modified,
                    Some(FileStatus::Deleted) => colors.version_control_deleted,
                    Some(FileStatus::Conflict) => colors.version_control_conflict,
                    None => colors.text,
                };
                let chevron = if row.is_dir {
                    Icon::new(if row.expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(IconSize::XSmall)
                    .color(Color::Muted)
                    .into_any_element()
                } else {
                    div().w(px(12.)).into_any_element()
                };
                let icon = match (row.is_dir, row.expanded) {
                    (true, true) => IconName::FolderOpen,
                    (true, false) => IconName::Folder,
                    _ => IconName::File,
                };
                let rel = row
                    .path
                    .strip_prefix(&root)
                    .unwrap_or(&row.path)
                    .to_string_lossy()
                    .replace('\\', "/");
                let (path, is_dir) = (row.path.clone(), row.is_dir);
                div()
                    .id(("file-row", i))
                    .debug_selector(move || format!("file-{rel}"))
                    .h(px(ROW_HEIGHT))
                    .mx_1()
                    .pl(px(6. + INDENT * row.depth as f32))
                    .pr_2()
                    .flex()
                    .items_center()
                    .gap_1()
                    .rounded(px(6.))
                    .when(selected, |r| r.bg(selected_bg))
                    .hover(move |s| if selected { s } else { s.bg(hover_bg) })
                    .child(chevron)
                    .child(Icon::new(icon).size(IconSize::Small).color(Color::Muted))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .truncate()
                            .text_size(px(13.))
                            .text_color(name_color)
                            .child(row.name.clone()),
                    )
                    .on_drag(self.dragged(&row.path), |dragged, _, _, cx| {
                        cx.new(|_| dragged.clone())
                    })
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        window.focus(&this.focus_handle, cx);
                        if event.modifiers().platform {
                            this.select(&path, true, cx);
                            return;
                        }
                        this.select(&path, false, cx);
                        match (is_dir, event.click_count()) {
                            // The second click of a double-click leaves the
                            // folder as the first one left it.
                            (true, 1) => this.toggle(&path, cx),
                            (false, 2) => this.open(&path, false, cx),
                            _ => {}
                        }
                    }))
                    .into_any_element()
            })
            .collect()
    }
}

impl Focusable for FilesView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for FilesView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = v_flex()
            .id("files")
            .key_context(KEY_CONTEXT)
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _: &SelectPrevious, _, cx| this.move_cursor(false, cx)))
            .on_action(cx.listener(|this, _: &SelectNext, _, cx| this.move_cursor(true, cx)))
            .on_action(cx.listener(|this, _: &ExpandSelected, _, cx| {
                this.expand_selected(true, cx);
            }))
            .on_action(cx.listener(|this, _: &CollapseSelected, _, cx| {
                this.expand_selected(false, cx);
            }))
            .on_action(cx.listener(|this, _: &OpenSelected, _, cx| this.open_selected(cx)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, _, window, cx| window.focus(&this.focus_handle, cx)),
            )
            .size_full();
        if self.tree.is_none() {
            return root.child(empty_state("No project open", ""));
        }
        if self.rows.is_empty() {
            return root.child(empty_state("Empty folder", ""));
        }
        root.child(
            uniform_list(
                "file-rows",
                self.rows.len(),
                cx.processor(|this, range, _, cx| this.render_rows(range, cx)),
            )
            .track_scroll(&self.scroll)
            .flex_1()
            .min_h_0()
            .pb_2(),
        )
    }
}

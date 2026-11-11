//! Opening, saving, closing and reloading files; the open-file / folder
//! dialogs; unsaved-changes prompts; quitting.

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumeditor_core::cli::OpenTarget;
use ferrumeditor_core::files::{self, LoadError};
use gpui::{PathPromptOptions, PromptLevel};

use super::{PaneItem, Workspace};
use crate::editor::EditorView;
use crate::tree_view::TreeView;

/// Runs after a successful save (e.g. closing the tab).
pub type AfterSave = Box<dyn FnOnce(&mut Workspace, &mut Window, &mut Context<Workspace>)>;

impl Workspace {
    /// Opens CLI targets: the first folder becomes the project root, files
    /// open in tabs at their line / column.
    pub fn open_targets(
        &mut self,
        targets: Vec<OpenTarget>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        for target in targets {
            if target.path.is_dir() {
                if self.root.is_none() {
                    self.set_root(target.path.clone(), window, cx);
                }
            } else {
                self.open_file_at(
                    &target.path,
                    target.line.map(|l| l - 1),
                    target.column.map(|c| c - 1),
                    window,
                    cx,
                );
            }
        }
    }

    pub fn set_root(&mut self, root: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        let root = root.canonicalize().unwrap_or(root);
        let tree = cx.new(|cx| TreeView::new(root.clone(), window, cx));
        let subscription = cx.subscribe_in(&tree, window, Self::on_tree_event);
        self._subscriptions.push(subscription);
        self.tree = Some(tree);
        self.tree_visible = true;
        self.root = Some(root.clone());
        self.file_list = None;
        self.project_find
            .update(cx, |pf, cx| pf.set_root(Some(root.clone()), cx));
        self.refresh_branch();
        self.start_watcher(window, cx);
        if window.focused(cx).is_none() {
            self.focus_active(window, cx);
        }
        self.sync_tree_selection(cx);
        cx.notify();
    }

    pub(super) fn refresh_branch(&mut self) {
        self.branch = self
            .root
            .as_deref()
            .and_then(ferrumeditor_core::git::repo_root)
            .and_then(|repo| ferrumeditor_core::git::current_branch(&repo));
    }

    /// Opens `path` (or activates its tab). A missing file opens as an empty
    /// buffer that will be created on save.
    pub fn open_file(
        &mut self,
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<EditorView>> {
        let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        for (p, pane) in self.panes.iter().enumerate() {
            for (i, item) in pane.items.iter().enumerate() {
                if let PaneItem::Editor(editor) = item
                    && editor.read(cx).path.as_deref() == Some(path.as_path())
                {
                    let editor = editor.clone();
                    if p == self.active_pane {
                        self.activate_item(p, i, window, cx);
                    } else {
                        self.add_item(PaneItem::Editor(editor.clone()), window, cx);
                    }
                    return Some(editor);
                }
            }
        }
        let editor = if path.exists() {
            match files::load(&path) {
                Ok(file) => cx.new(|cx| EditorView::from_file(file, path.clone(), window, cx)),
                Err(error) => {
                    let message = match error {
                        LoadError::Binary => "is a binary file".to_string(),
                        other => other.to_string(),
                    };
                    self.error(format!("Can't open {}: {message}", path.display()), cx);
                    return None;
                }
            }
        } else {
            cx.new(|cx| EditorView::new("", Some(path.clone()), window, cx))
        };
        self.add_item(PaneItem::Editor(editor.clone()), window, cx);
        Some(editor)
    }

    /// Opens a file and moves the caret to 0-based `line` / `column`.
    pub fn open_file_at(
        &mut self,
        path: &Path,
        line: Option<usize>,
        column: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.open_file(path, window, cx) else {
            return;
        };
        if let Some(line) = line {
            editor.update(cx, |editor, cx| {
                editor.state.go_to_point(line, column.unwrap_or(0));
                editor.scroll_to_line(line, cx);
            });
        }
    }

    pub fn new_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let editor = cx.new(|cx| EditorView::new("", None, window, cx));
        self.add_item(PaneItem::Editor(editor), window, cx);
    }

    pub fn open_file_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt_paths(false, window, cx);
    }

    pub fn open_folder_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt_paths(true, window, cx);
    }

    fn prompt_paths(&mut self, folders: bool, window: &mut Window, cx: &mut Context<Self>) {
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: !folders,
            directories: folders,
            multiple: !folders,
            prompt: Some(if folders { "Open Folder" } else { "Open" }.into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                for path in paths {
                    if path.is_dir() {
                        this.set_root(path, window, cx);
                    } else {
                        this.open_file(&path, window, cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Saves `editor`; untitled buffers (or `save_as`) ask for a path.
    /// `then` runs after a successful save.
    pub fn save_editor(
        &mut self,
        editor: Entity<EditorView>,
        save_as: bool,
        then: Option<AfterSave>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = editor.read(cx).path.clone();
        if let (Some(_), false) = (&current, save_as) {
            match editor.update(cx, |editor, cx| editor.save(None, cx)) {
                Ok(()) => {
                    if let Some(then) = then {
                        then(self, window, cx);
                    }
                }
                Err(error) => self.error(format!("Save failed: {error}"), cx),
            }
            cx.notify();
            return;
        }
        let directory = current
            .as_deref()
            .and_then(Path::parent)
            .map(Path::to_path_buf)
            .or_else(|| self.root.clone())
            .unwrap_or_else(|| PathBuf::from("."));
        let suggested = current
            .as_deref()
            .and_then(Path::file_name)
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "untitled.txt".into());
        let receiver = cx.prompt_for_new_path(&directory, Some(&suggested));
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            this.update_in(cx, |this, window, cx| {
                match editor.update(cx, |editor, cx| editor.save(Some(path), cx)) {
                    Ok(()) => {
                        if let Some(then) = then {
                            then(this, window, cx);
                        }
                    }
                    Err(error) => this.error(format!("Save failed: {error}"), cx),
                }
                this.sync_tree_selection(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn save_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for editor in self.all_editors() {
            let (dirty, has_path) = {
                let e = editor.read(cx);
                (e.is_dirty(), e.path.is_some())
            };
            if dirty && has_path {
                self.save_editor(editor, false, None, window, cx);
            }
        }
    }

    /// Closes an item, asking first if it is the last view of an editor with
    /// unsaved changes.
    pub fn request_close_item(
        &mut self,
        pane: usize,
        index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(item) = self
            .panes
            .get(pane)
            .and_then(|p| p.items.get(index))
            .cloned()
        else {
            return;
        };
        let PaneItem::Editor(editor) = &item else {
            self.remove_item(pane, index, window, cx);
            return;
        };
        let views = self
            .panes
            .iter()
            .flat_map(|p| p.items.iter())
            .filter(|i| matches!(i, PaneItem::Editor(e) if e == editor))
            .count();
        if !editor.read(cx).is_dirty() || views > 1 {
            self.remove_item(pane, index, window, cx);
            return;
        }
        let name = editor.read(cx).title();
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!("Save changes to “{name}” before closing?"),
            Some("Your changes will be lost if you don't save them."),
            &["Save", "Don't Save", "Cancel"],
            cx,
        );
        let editor = editor.clone();
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = answer.await else { return };
            this.update_in(cx, |this, window, cx| match answer {
                0 => {
                    let target = editor.clone();
                    let close = Box::new(
                        move |this: &mut Self, window: &mut Window, cx: &mut Context<Self>| {
                            if let Some((p, i)) = this.locate(&target) {
                                this.remove_item(p, i, window, cx);
                            }
                        },
                    );
                    this.save_editor(editor.clone(), false, Some(close), window, cx);
                }
                1 => {
                    if let Some((p, i)) = this.locate(&editor) {
                        this.remove_item(p, i, window, cx);
                    }
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn locate(&self, editor: &Entity<EditorView>) -> Option<(usize, usize)> {
        self.panes.iter().enumerate().find_map(|(p, pane)| {
            pane.items
                .iter()
                .position(|i| matches!(i, PaneItem::Editor(e) if e == editor))
                .map(|i| (p, i))
        })
    }

    pub fn close_active_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let pane = self.active_pane;
        if self.panes[pane].items.is_empty() {
            self.close_pane(pane, window, cx);
        } else {
            let index = self.panes[pane].active;
            self.request_close_item(pane, index, window, cx);
        }
    }

    pub fn reopen_closed_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(path) = self.closed_tabs.pop() {
            self.open_file(&path, window, cx);
        }
    }

    /// Reloads every open file whose disk stamp changed: silently when it
    /// has no unsaved edits, otherwise after asking.
    pub fn check_disk_changes(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        for editor in self.all_editors() {
            let (path, stamp, dirty) = {
                let e = editor.read(cx);
                (e.path.clone(), e.disk_stamp, e.is_dirty())
            };
            let Some(path) = path else { continue };
            let current = files::disk_stamp(&path);
            if current.is_none() || current == stamp {
                continue;
            }
            let ask = dirty
                && cx
                    .global::<crate::settings::Settings>()
                    .config
                    .confirm_reload_dirty;
            if !ask {
                if let Err(error) = editor.update(cx, |e, cx| e.reload_from_disk(cx)) {
                    self.error(format!("Reload failed: {error}"), cx);
                }
                continue;
            }
            // Ask once per change: remember the new stamp now.
            editor.update(cx, |e, _| e.disk_stamp = current);
            let name = editor.read(cx).title();
            let answer = window.prompt(
                PromptLevel::Warning,
                &format!("“{name}” changed on disk."),
                Some("Reload it and lose your unsaved changes, or keep your version?"),
                &["Reload", "Keep My Changes"],
                cx,
            );
            cx.spawn_in(window, async move |this, cx| {
                if answer.await == Ok(0) {
                    this.update_in(cx, |this, _, cx| {
                        if let Err(error) = editor.update(cx, |e, cx| e.reload_from_disk(cx)) {
                            this.error(format!("Reload failed: {error}"), cx);
                        }
                    })
                    .ok();
                }
            })
            .detach();
            // One prompt at a time; the next change check picks up the rest.
            break;
        }
    }

    /// Quits after saving the session, asking about unsaved changes.
    pub fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let dirty: Vec<Entity<EditorView>> = self
            .all_editors()
            .into_iter()
            .filter(|e| e.read(cx).is_dirty())
            .collect();
        if dirty.is_empty() {
            self.save_session(cx);
            cx.quit();
            return;
        }
        let answer = window.prompt(
            PromptLevel::Warning,
            &format!(
                "You have unsaved changes in {} file{}.",
                dirty.len(),
                if dirty.len() == 1 { "" } else { "s" }
            ),
            Some("Save them before quitting?"),
            &["Save All", "Don't Save", "Cancel"],
            cx,
        );
        cx.spawn_in(window, async move |this, cx| {
            let Ok(answer) = answer.await else { return };
            this.update_in(cx, |this, window, cx| match answer {
                0 => {
                    if dirty.iter().any(|e| e.read(cx).path.is_none()) {
                        this.error("Save or close untitled files first", cx);
                        return;
                    }
                    this.save_all(window, cx);
                    this.save_session(cx);
                    cx.quit();
                }
                1 => {
                    this.save_session(cx);
                    cx.quit();
                }
                _ => {}
            })
            .ok();
        })
        .detach();
    }
}

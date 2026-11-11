//! Pickers (command palette, file finder, go to line, theme selectors), the find bar,
//! find in files, and the Markdown preview toggle.

use std::sync::Arc;

use ferrum_ui::prelude::*;
use ferrumeditor_core::file_tree::list_project_files;
use ferrumeditor_core::keymap::display_keys;
use ferrumeditor_core::language::Language;
use ferrumeditor_core::session::SplitAxis;

use super::{PaneItem, Workspace};
use crate::find_bar::FindBarEvent;
use crate::markdown_preview::MarkdownPreview;
use crate::picker::{Picker, PickerEvent, PickerItem, PickerKind};
use crate::project_find::ProjectFindEvent;
use crate::settings::{MAC, Settings};

/// Most files listed by the fuzzy finder.
const FILE_LIMIT: usize = 100_000;

impl Workspace {
    pub(super) fn toggle_picker(
        &mut self,
        kind: PickerKind,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.picker.is_some() {
            self.close_picker(window, cx);
            return;
        }
        let (items, placeholder) = match kind {
            PickerKind::Commands => (self.command_items(cx), "Run a command…"),
            PickerKind::Files => {
                if self.root.is_none() && self.all_editors().is_empty() {
                    self.info("Open a folder first (⌘⇧O)", cx);
                    return;
                }
                if self.file_list.is_none() {
                    self.load_file_list(cx);
                }
                (self.file_items(cx), "Go to file…")
            }
            PickerKind::GoToLine => {
                if self.active_editor().is_none() {
                    return;
                }
                (Vec::new(), "Line[:column]")
            }
            PickerKind::Theme | PickerKind::IconTheme => Self::theme_items(kind, cx),
            PickerKind::Problems => (self.problem_items(cx), "Go to problem…"),
            PickerKind::Locations => (
                std::mem::take(&mut self.location_items),
                "Go to definition…",
            ),
            PickerKind::CodeActions => (
                std::mem::take(&mut self.location_items),
                "Pick a code action…",
            ),
            PickerKind::Agents => match self.agent_items(cx) {
                Some(items) => (items, "Start an agent session…"),
                None => return,
            },
            PickerKind::DebugConfigs => match self.debug_items(cx) {
                Some(items) => (items, "Start debugging…"),
                None => return,
            },
            PickerKind::Tasks => {
                let items = self.task_items();
                if items.is_empty() {
                    self.info(
                        "No tasks found: open a folder with Cargo.toml, go.mod, package.json, a Makefile or .vscode/tasks.json",
                        cx,
                    );
                    return;
                }
                (items, "Run a task…")
            }
        };
        let picker = cx.new(|cx| Picker::new(kind, items, placeholder, window, cx));
        if matches!(kind, PickerKind::Theme | PickerKind::IconTheme) {
            let current = self.begin_theme_preview(kind, cx);
            picker.update(cx, |picker, cx| picker.select_payload(&current, cx));
        }
        let subscription = cx.subscribe_in(&picker, window, Self::on_picker_event);
        self._subscriptions.push(subscription);
        self.picker = Some(picker);
        cx.notify();
    }

    /// Closes the picker; an unconfirmed theme preview is reverted.
    pub(super) fn close_picker(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.picker = None;
        self.revert_theme_preview(cx);
        self.focus_active(window, cx);
        cx.notify();
    }

    fn command_items(&self, cx: &App) -> Vec<PickerItem> {
        let settings = cx.global::<Settings>();
        let mut items: Vec<PickerItem> = settings
            .commands
            .iter()
            .filter(|c| {
                !c.name.starts_with("editor:move-") && !c.name.starts_with("editor:select-to")
            })
            .map(|c| {
                PickerItem::new(
                    c.title(),
                    settings
                        .keymap
                        .keys_for(&c.name)
                        .first()
                        .map(|k| display_keys(k, MAC))
                        .unwrap_or_default(),
                    c.name.clone(),
                )
            })
            .collect();
        items.sort_by(|a, b| a.label.cmp(&b.label));
        items
    }

    /// Walks the project on a background thread; the open file finder
    /// picks the list up when it arrives.
    fn load_file_list(&mut self, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        self.file_list_task = Some(cx.spawn(async move |this, cx| {
            let list = cx
                .background_executor()
                .spawn(async move { list_project_files(&root, FILE_LIMIT) })
                .await;
            this.update(cx, |this, cx| {
                this.file_list = Some(Arc::new(list));
                if let Some(picker) = this.picker.clone()
                    && picker.read(cx).kind() == PickerKind::Files
                {
                    let items = this.file_items(cx);
                    picker.update(cx, |picker, cx| picker.set_items(items, cx));
                }
            })
            .ok();
        }));
    }

    fn file_items(&self, cx: &App) -> Vec<PickerItem> {
        let mut items = Vec::new();
        if let (Some(root), Some(files)) = (self.root.clone(), self.file_list.clone()) {
            items.extend(files.iter().map(|rel| {
                let label = rel.to_string_lossy().to_string();
                let path = root.join(rel);
                let mut item =
                    PickerItem::new(label, SharedString::default(), path.to_string_lossy());
                item.icon = Some(path);
                item
            }));
        }
        // Open files outside the project are findable too.
        for editor in self.all_editors() {
            if let Some(path) = editor.read(cx).path.clone()
                && self
                    .root
                    .as_deref()
                    .is_none_or(|root| !path.starts_with(root))
            {
                let mut item = PickerItem::new(
                    path.to_string_lossy().to_string(),
                    "open",
                    path.to_string_lossy(),
                );
                item.icon = Some(path);
                items.push(item);
            }
        }
        items
    }

    fn on_picker_event(
        &mut self,
        _: &Entity<Picker>,
        event: &PickerEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (kind, payload) = match event {
            PickerEvent::Highlighted { kind, payload } => {
                if self.theme_preview.as_ref().is_some_and(|(k, _)| k == kind) {
                    self.preview_theme(*kind, payload, cx);
                }
                return;
            }
            PickerEvent::Dismissed => {
                self.close_picker(window, cx);
                return;
            }
            PickerEvent::Confirmed { kind, payload } => (*kind, payload.clone()),
        };
        if matches!(kind, PickerKind::Theme | PickerKind::IconTheme) {
            self.confirm_theme(kind, &payload, cx);
        }
        self.close_picker(window, cx);
        match kind {
            PickerKind::Commands => {
                let name: &str = &payload;
                if name.starts_with("editor:") && self.active_editor().is_some() {
                    if let Some(editor) = self.active_editor() {
                        editor.update(cx, |e, cx| e.run_command(name, window, cx));
                    }
                } else {
                    self.run_command(name, window, cx);
                }
            }
            PickerKind::Files => {
                self.open_file(std::path::Path::new(&payload), window, cx);
            }
            PickerKind::GoToLine => {
                let mut parts = payload.trim().split(':');
                let line = parts.next().and_then(|l| l.trim().parse::<usize>().ok());
                let column = parts.next().and_then(|c| c.trim().parse::<usize>().ok());
                if let (Some(line), Some(editor)) = (line, self.active_editor()) {
                    editor.update(cx, |e, cx| {
                        let line = line.saturating_sub(1);
                        e.state
                            .go_to_point(line, column.unwrap_or(1).saturating_sub(1));
                        e.scroll_to_line(line, cx);
                    });
                }
            }
            PickerKind::Tasks => self.run_task_payload(&payload, window, cx),
            PickerKind::Problems | PickerKind::Locations => {
                self.open_location_payload(&payload, window, cx);
            }
            PickerKind::CodeActions => self.run_code_action_payload(&payload, cx),
            PickerKind::Agents => self.agent_payload(&payload, window, cx),
            PickerKind::DebugConfigs => self.debug_payload(&payload, cx),
            PickerKind::Theme | PickerKind::IconTheme => {}
        }
    }

    pub(super) fn show_find_bar(
        &mut self,
        replace: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prefill = self
            .active_editor()
            .and_then(|e| e.read(cx).selected_text());
        self.find_visible = true;
        self.find_bar.update(cx, |bar, cx| {
            if replace {
                bar.show_replace = true;
            }
            bar.focus_find(prefill, window, cx);
            if replace && !bar.find.read(cx).text().is_empty() {
                bar.focus_replace(window, cx);
            }
        });
        cx.notify();
    }

    pub(super) fn hide_find_bar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.find_visible = false;
        for editor in self.all_editors() {
            editor.update(cx, |e, cx| e.clear_search(cx));
        }
        self.focus_active(window, cx);
        cx.notify();
    }

    pub(super) fn find_in_active(&mut self, forward: bool, _: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        let query = self.find_bar.read(cx).query(cx);
        if query.pattern.is_empty() {
            return;
        }
        editor.update(cx, |e, cx| {
            if e.search.query.as_ref() != Some(&query) {
                e.set_search(Some(query), false, cx);
            }
            e.find_next(forward, cx);
        });
        self.update_find_status(cx);
    }

    pub(super) fn use_selection_as_find(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self
            .active_editor()
            .and_then(|e| e.read(cx).selected_text())
        else {
            return;
        };
        self.find_bar.update(cx, |bar, cx| {
            bar.find.update(cx, |input, cx| input.set_text(text, cx))
        });
        let query = self.find_bar.read(cx).query(cx);
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |e, cx| e.set_search(Some(query), false, cx));
        }
        self.update_find_status(cx);
    }

    fn update_find_status(&mut self, cx: &mut Context<Self>) {
        let status: SharedString = match self.active_editor() {
            Some(editor) => {
                let e = editor.read(cx);
                match (&e.search.error, e.search_summary()) {
                    (Some(error), _) => error.clone().into(),
                    (None, (_, 0)) if e.search.is_active() => "No results".into(),
                    (None, (Some(active), total)) => format!("{} of {total}", active + 1).into(),
                    (None, (None, total)) if total > 0 => format!("{total} results").into(),
                    _ => SharedString::default(),
                }
            }
            None => SharedString::default(),
        };
        self.find_bar.update(cx, |bar, cx| {
            bar.status = status;
            cx.notify();
        });
    }

    pub(super) fn on_find_bar_event(
        &mut self,
        _: &Entity<crate::find_bar::FindBar>,
        event: &FindBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self.active_editor() else {
            if matches!(event, FindBarEvent::Close) {
                self.hide_find_bar(window, cx);
            }
            return;
        };
        let replacement = self.find_bar.read(cx).replacement(cx);
        match event {
            FindBarEvent::QueryChanged => {
                let query = self.find_bar.read(cx).query(cx);
                editor.update(cx, |e, cx| e.set_search(Some(query), true, cx));
            }
            FindBarEvent::Next => self.find_in_active(true, window, cx),
            FindBarEvent::Previous => self.find_in_active(false, window, cx),
            FindBarEvent::Replace => editor.update(cx, |e, cx| e.replace_next(&replacement, cx)),
            FindBarEvent::ReplaceAll => {
                let count = editor.update(cx, |e, cx| e.replace_all(&replacement, cx));
                self.info(
                    format!(
                        "Replaced {count} occurrence{}",
                        if count == 1 { "" } else { "s" }
                    ),
                    cx,
                );
            }
            FindBarEvent::SelectAll => {
                editor.update(cx, |e, cx| e.select_all_matches(cx));
                self.focus_active(window, cx);
            }
            FindBarEvent::Close => {
                self.hide_find_bar(window, cx);
                return;
            }
        }
        self.update_find_status(cx);
    }

    pub(super) fn show_project_find(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prefill = self
            .active_editor()
            .and_then(|e| e.read(cx).selected_text());
        self.project_find_visible = true;
        self.project_find
            .update(cx, |pf, cx| pf.focus(prefill, window, cx));
        cx.notify();
    }

    pub(super) fn on_project_find_event(
        &mut self,
        _: &Entity<crate::project_find::ProjectFind>,
        event: &ProjectFindEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ProjectFindEvent::Open { path, line, column } => {
                self.open_file_at(path, Some(*line), Some(*column), window, cx);
            }
            ProjectFindEvent::Close => {
                self.project_find_visible = false;
                self.focus_active(window, cx);
                cx.notify();
            }
        }
    }

    /// Opens a preview of the active Markdown editor in the next pane
    /// (splitting right if needed), or closes its existing preview.
    pub(super) fn toggle_markdown_preview(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.active_editor() else {
            return;
        };
        for p in 0..self.panes.len() {
            if let Some(i) = self.panes[p]
                .items
                .iter()
                .position(|item| matches!(item, PaneItem::Preview(_, source) if *source == editor))
            {
                self.remove_item(p, i, window, cx);
                return;
            }
        }
        if editor.read(cx).language() != Language::Markdown {
            self.info("Markdown preview works on Markdown files", cx);
            return;
        }
        let preview = cx.new(|cx| MarkdownPreview::new(editor.clone(), window, cx));
        let source_pane = self.active_pane;
        if self.panes.len() == 1 {
            self.split(SplitAxis::Horizontal, window, cx);
            // `split` cloned the editor into the new pane; show the preview instead.
            self.panes[self.active_pane].items.clear();
        } else {
            let order = self.layout.leaves();
            let current = self.panes[source_pane].id;
            let pos = order.iter().position(|id| *id == current).unwrap_or(0);
            let next = order[(pos + 1) % order.len()];
            self.active_pane = self.pane_index(next).unwrap_or(source_pane);
        }
        self.add_item(PaneItem::Preview(preview, editor), window, cx);
        // Keep typing in the source.
        self.active_pane = source_pane;
        self.focus_active(window, cx);
    }
}

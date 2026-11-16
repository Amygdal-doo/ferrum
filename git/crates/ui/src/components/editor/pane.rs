//! The editor in the detail card of the stand-alone app: a header (path,
//! unsaved dot, Find / Save / close) over a [`CodeEditor`]. One file is open
//! at a time; work is never dropped silently: closing it, opening another
//! file or quitting with unsaved changes asks first (Save / Discard /
//! Cancel). Saving reloads the repository, so the Changes list and the diff
//! follow. Hosts (`open_files_in_host`) get `GitViewEvent::OpenFile` instead.

use super::{CodeEditor, CodeEditorEvent};
use crate::app::GitView;
use crate::components::icons::Icon;
use crate::components::widgets::{ghost_button, primary_button, small_button};
use ferrum_ui::components::{
    ButtonCommon, Clickable, Color, Disableable, IconSize, Label, LabelCommon, LabelSize,
    StyledTypography, Tooltip, h_flex, v_flex,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumgit_domain::RepoId;
use gpui::{
    AnyElement, AppContext as _, Context, Entity, Subscription, Window, div, prelude::*, px,
};
use std::path::Path;

/// A working-tree file open in the editor.
pub struct OpenEditor {
    pub repo: RepoId,
    /// Path relative to the repository's work tree.
    pub path: String,
    pub view: Entity<CodeEditor>,
    /// The dirty state the header shows (it re-renders when this flips).
    dirty: bool,
    /// Waiting for an answer about unsaved changes before doing this.
    pub pending: Option<Pending>,
    _events: Subscription,
}

/// What asked to leave a file with unsaved changes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    Close,
    Open { path: String, line: Option<u32> },
    Quit,
}

/// The answer to the unsaved-changes question.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    Save,
    Discard,
    Cancel,
}

impl GitView {
    /// "Edit" / "Open" on a working-tree file: the host's editor when
    /// embedded, else the built-in one at 1-based `line`.
    pub(crate) fn edit_file(
        &mut self,
        id: RepoId,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.options.open_files_in_host {
            self.open_in_host(id, path, line, cx);
        } else {
            self.open_in_editor(id, path, line, window, cx);
        }
    }

    /// Opens `path` (relative to repository `id`) in the built-in editor.
    /// Another file with unsaved changes is asked about first.
    pub fn open_in_editor(
        &mut self,
        id: RepoId,
        path: &str,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(open) = &mut self.editor {
            if open.repo == id && open.path == path {
                open.pending = None;
                let view = open.view.clone();
                view.update(cx, |editor, cx| {
                    if let Some(line) = line {
                        editor.go_to_line(line as usize, cx);
                    }
                    editor.focus(window, cx);
                });
                cx.notify();
                return;
            }
            if open.view.read(cx).is_dirty() {
                open.pending = Some(Pending::Open {
                    path: path.to_owned(),
                    line,
                });
                cx.notify();
                return;
            }
        }
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        let full = Path::new(&entry.path).join(path);
        let file = match CodeEditor::load(&full) {
            Ok(file) => file,
            Err(e) => {
                self.notify(e, true, cx);
                return;
            }
        };
        let view = cx.new(|cx| CodeEditor::new(file, full, cx));
        let events = cx.subscribe(&view, Self::on_editor_event);
        view.update(cx, |editor, cx| {
            if let Some(line) = line {
                editor.go_to_line(line as usize, cx);
            }
            editor.focus(window, cx);
        });
        self.editor = Some(OpenEditor {
            repo: id,
            path: path.to_owned(),
            view,
            dirty: false,
            pending: None,
            _events: events,
        });
        cx.notify();
    }

    fn on_editor_event(
        &mut self,
        view: Entity<CodeEditor>,
        event: &CodeEditorEvent,
        cx: &mut Context<Self>,
    ) {
        let dirty = view.read(cx).is_dirty();
        let Some(open) = self.editor.as_mut().filter(|o| o.view == view) else {
            return;
        };
        let repo = open.repo;
        if std::mem::replace(&mut open.dirty, dirty) != dirty {
            cx.notify();
        }
        match event {
            CodeEditorEvent::Edited => {}
            // The Changes list and the open diff follow the file.
            CodeEditorEvent::Saved => self.load(repo, cx),
            CodeEditorEvent::Error(message) => self.notify(message.clone(), true, cx),
        }
    }

    /// The editor's file has unsaved changes.
    #[must_use]
    pub fn has_unsaved_edits(&self, cx: &gpui::App) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|o| o.view.read(cx).is_dirty())
    }

    /// The editor covers the detail card (its repository is shown).
    pub(crate) fn editor_shown(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|o| Some(o.repo) == self.state.active)
    }

    /// Closes the editor, asking first when it has unsaved changes.
    pub fn close_editor(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.editor else { return };
        if open.view.read(cx).is_dirty() {
            open.pending = Some(Pending::Close);
        } else {
            self.editor = None;
            self.focus_root(window, cx);
        }
        cx.notify();
    }

    /// Whether the window may close now; with unsaved changes it asks
    /// instead (and quits once they are saved or discarded).
    pub fn confirm_quit(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(open) = &mut self.editor else {
            return true;
        };
        if !open.view.read(cx).is_dirty() {
            return true;
        }
        open.pending = Some(Pending::Quit);
        if Some(open.repo) != self.state.active {
            let repo = open.repo;
            self.activate(Some(repo), cx);
        }
        cx.notify();
        false
    }

    pub(crate) fn on_quit(&mut self, _: &ferrum_ui::Quit, _: &mut Window, cx: &mut Context<Self>) {
        if self.confirm_quit(cx) {
            cx.propagate();
        }
    }

    /// Acts on the answer to the unsaved-changes bar.
    pub fn answer_unsaved(&mut self, answer: Answer, window: &mut Window, cx: &mut Context<Self>) {
        let Some(open) = &mut self.editor else { return };
        let Some(pending) = open.pending.take() else {
            return;
        };
        let view = open.view.clone();
        let repo = open.repo;
        match answer {
            Answer::Cancel => {
                view.update(cx, |editor, cx| editor.focus(window, cx));
                cx.notify();
                return;
            }
            Answer::Save => {
                if view.update(cx, |editor, cx| editor.save(cx)).is_err() {
                    return;
                }
            }
            Answer::Discard => {}
        }
        self.editor = None;
        match pending {
            Pending::Close => self.focus_root(window, cx),
            Pending::Open { path, line } => self.open_in_editor(repo, &path, line, window, cx),
            Pending::Quit => cx.quit(),
        }
        cx.notify();
    }

    /// Before repository `id` closes: drops its editor, or, with unsaved
    /// changes, shows it and asks instead (false: keep the repository open).
    pub(crate) fn release_editor(&mut self, id: RepoId, cx: &mut Context<Self>) -> bool {
        let Some(open) = self.editor.as_mut().filter(|o| o.repo == id) else {
            return true;
        };
        if !open.view.read(cx).is_dirty() {
            self.editor = None;
            return true;
        }
        open.pending = Some(Pending::Close);
        if self.state.active != Some(id) {
            self.activate(Some(id), cx);
        }
        cx.notify();
        false
    }

    /// After a reload: re-reads the file if it changed on disk and has no
    /// edits of its own (a discard or checkout from FerrumGIT or elsewhere).
    pub(crate) fn refresh_editor(&mut self, id: RepoId, cx: &mut Context<Self>) {
        if let Some(open) = self.editor.as_ref().filter(|o| o.repo == id) {
            open.view
                .update(cx, |editor, cx| editor.reload_if_changed(cx));
        }
    }
}

/// The detail card while the editor is shown.
pub fn render(app: &GitView, cx: &mut Context<GitView>) -> AnyElement {
    let Some(open) = &app.editor else {
        return div().into_any_element();
    };
    let theme = cx.theme().clone();
    let colors = theme.colors();
    let dirty = open.view.read(cx).is_dirty();
    let view = open.view.clone();
    let find_view = view.clone();
    let header = h_flex()
        .flex_none()
        .h(px(36.))
        .gap_2()
        .px_4()
        .border_b_1()
        .border_color(colors.border)
        .text_ui_sm(cx)
        .child(Icon::File.icon().size(IconSize::Small).color(Color::Muted))
        .child(
            div()
                .min_w_0()
                .truncate()
                .font_weight(gpui::FontWeight::MEDIUM)
                .child(open.path.clone()),
        )
        .when(dirty, |d| {
            d.child(
                div()
                    .id("editor-dirty")
                    .flex_none()
                    .size(px(8.))
                    .rounded_full()
                    .bg(colors.text_accent)
                    .tooltip(Tooltip::text("Unsaved changes")),
            )
        })
        .child(div().flex_1())
        .child(
            ghost_button("editor-find", "Find")
                .tooltip(Tooltip::text("Find in this file (⌘F)"))
                .on_click(cx.listener(move |_, _, window, cx| {
                    find_view.update(cx, |editor, cx| editor.open_find(window, cx));
                })),
        )
        .child(
            primary_button("editor-save", "Save")
                .disabled(!dirty)
                .tooltip(Tooltip::text("Save (⌘S)"))
                .on_click(cx.listener(move |_, _, _, cx| {
                    view.update(cx, |editor, cx| editor.save(cx)).ok();
                })),
        )
        .child(
            div()
                .id("editor-close")
                .ml_1()
                .p_1()
                .rounded_sm()
                .cursor_pointer()
                .hover(|s| s.bg(colors.element_hover))
                .tooltip(Tooltip::text("Close the editor (Esc)"))
                .child(Icon::Close.icon().size(IconSize::Small).color(Color::Muted))
                .on_click(cx.listener(|app, _, window, cx| app.close_editor(window, cx))),
        );
    v_flex()
        .size_full()
        .child(header)
        .children(
            open.pending
                .as_ref()
                .map(|p| unsaved_bar(&open.path, p, cx)),
        )
        .child(div().flex_1().min_h_0().child(open.view.clone()))
        .into_any_element()
}

/// "Save changes?" between the header and the text.
fn unsaved_bar(path: &str, pending: &Pending, cx: &mut Context<GitView>) -> AnyElement {
    let status = cx.theme().status();
    let what = match pending {
        Pending::Close => "before closing",
        Pending::Open { .. } => "before opening another file",
        Pending::Quit => "before quitting",
    };
    h_flex()
        .flex_none()
        .gap_2()
        .px_4()
        .py_2()
        .border_b_1()
        .border_color(status.warning_border)
        .bg(status.warning_background)
        .child(div().flex_1().min_w_0().child(
            Label::new(format!("Save your changes to “{path}” {what}?")).size(LabelSize::Small),
        ))
        .child(small_button("unsaved-cancel", "Cancel").on_click(answer(Answer::Cancel, cx)))
        .child(
            ghost_button("unsaved-discard", "Discard")
                .color(Color::Error)
                .on_click(answer(Answer::Discard, cx)),
        )
        .child(primary_button("unsaved-save", "Save").on_click(answer(Answer::Save, cx)))
        .into_any_element()
}

fn answer(
    answer: Answer,
    cx: &mut Context<GitView>,
) -> impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static {
    cx.listener(move |app, _, window, cx| app.answer_unsaved(answer, window, cx))
}

//! The Editor column: files opened from the chat, git or the Files pane,
//! one tab each. Text files are edited with FerrumGIT's editor
//! (`ferrumgit_ui::CodeEditor`: FerrumIDE's buffer, undo, highlighting and
//! keys, without language servers); images are drawn; other files are
//! described. Every tab has **Open in FerrumIDE**. Closing a tab with
//! unsaved changes asks first (Save / Discard / Cancel). The header's ⋯
//! menu moves or closes the column; the window handles those
//! ([`EditorEvent`]).

mod doc;

use std::path::{Path, PathBuf};

use ferrum_ui::components::IconSize;
use ferrum_ui::gpui::{ObjectFit, StyledImage, img};
use ferrum_ui::prelude::*;
use ferrumgit_ui::{CodeEditor, CodeEditorEvent};

pub use doc::{Kind, kind_of};

/// What the window does for the editor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    /// Open this file in FerrumIDE.
    OpenInIde { path: PathBuf, line: Option<u32> },
    /// Move the column one place left (`-1`) or right (`1`).
    Move(isize),
    /// Hide the column.
    Close,
    /// Saving failed.
    Error(String),
}

/// An open file's content.
enum Content {
    Text(Entity<CodeEditor>, #[allow(dead_code)] Subscription),
    Image,
    /// Not shown: binary, not UTF-8, too big, or unreadable.
    Other {
        reason: String,
    },
}

struct Tab {
    path: PathBuf,
    content: Content,
}

pub struct EditorView {
    tabs: Vec<Tab>,
    active: usize,
    /// The tab whose close waits for Save / Discard / Cancel.
    confirm_close: Option<PathBuf>,
    /// Whether the column can move left / right (set by the window).
    pub(crate) can_move: (bool, bool),
    /// The whole column (tabs, editors, messages).
    pub(crate) focus_handle: FocusHandle,
}

impl EventEmitter<EditorEvent> for EditorView {}

impl EditorView {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            tabs: Vec::new(),
            active: 0,
            confirm_close: None,
            can_move: (false, false),
            focus_handle: cx.focus_handle(),
        }
    }

    /// The open files, in tab order.
    #[cfg(test)]
    pub fn paths(&self) -> Vec<PathBuf> {
        self.tabs.iter().map(|t| t.path.clone()).collect()
    }

    /// The file shown.
    pub fn active_path(&self) -> Option<&Path> {
        self.tabs.get(self.active).map(|t| t.path.as_path())
    }

    /// The shown file's text editor, if it is text.
    pub fn active_editor(&self) -> Option<Entity<CodeEditor>> {
        match &self.tabs.get(self.active)?.content {
            Content::Text(editor, _) => Some(editor.clone()),
            _ => None,
        }
    }

    /// Shows `path` (opening a tab if needed) at 1-based `line`.
    pub fn open(
        &mut self,
        path: &Path,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let at = match self.tabs.iter().position(|t| t.path == path) {
            Some(at) => at,
            None => {
                let content = self.load(path, cx);
                self.tabs.push(Tab {
                    path: path.to_path_buf(),
                    content,
                });
                self.tabs.len() - 1
            }
        };
        self.active = at;
        match &self.tabs[at].content {
            Content::Text(editor, _) => editor.update(cx, |editor, cx| {
                if let Some(line) = line {
                    editor.go_to_line(line as usize, cx);
                }
                editor.focus(window, cx);
            }),
            _ => window.focus(&self.focus_handle, cx),
        }
        cx.notify();
    }

    fn load(&mut self, path: &Path, cx: &mut Context<Self>) -> Content {
        match kind_of(path) {
            Kind::Image => Content::Image,
            Kind::Binary => Content::Other {
                reason: "This file can't be read.".into(),
            },
            Kind::Text => match CodeEditor::load(path) {
                Ok(file) => {
                    let editor = cx.new(|cx| CodeEditor::new(file, path.to_path_buf(), cx));
                    let events = cx.subscribe(&editor, |_, _, event, cx| match event {
                        CodeEditorEvent::Error(error) => cx.emit(EditorEvent::Error(error.clone())),
                        CodeEditorEvent::Edited | CodeEditorEvent::Saved => cx.notify(),
                    });
                    Content::Text(editor, events)
                }
                Err(_) => {
                    let size = std::fs::metadata(path).map_or(0, |m| m.len());
                    Content::Other {
                        reason: format!(
                            "Binary or not UTF-8 text · {}. Open it in FerrumIDE or another app.",
                            doc::format_size(size)
                        ),
                    }
                }
            },
        }
    }

    /// Re-reads open files changed on disk (the agent wrote them) that have
    /// no edits of their own.
    pub fn reload_changed(&mut self, cx: &mut Context<Self>) {
        for tab in &self.tabs {
            if let Content::Text(editor, _) = &tab.content {
                editor.update(cx, |editor, cx| editor.reload_if_changed(cx));
            }
        }
    }

    /// Saves the shown file.
    pub fn save_active(&mut self, cx: &mut Context<Self>) {
        if let Some(editor) = self.active_editor() {
            editor.update(cx, |editor, cx| editor.save(cx)).ok();
        }
    }

    /// Closes `path`'s tab, asking first when it has unsaved changes.
    pub fn close(&mut self, path: &Path, cx: &mut Context<Self>) {
        let Some(at) = self.tabs.iter().position(|t| t.path == path) else {
            return;
        };
        if is_dirty(&self.tabs[at], cx) {
            self.active = at;
            self.confirm_close = Some(path.to_path_buf());
        } else {
            self.remove(at);
        }
        cx.notify();
    }

    /// The answer to "Save changes?": `Some(true)` saves then closes,
    /// `Some(false)` discards, `None` keeps the tab.
    pub fn answer_close(&mut self, save: Option<bool>, cx: &mut Context<Self>) {
        let Some(path) = self.confirm_close.take() else {
            return;
        };
        let Some(at) = self.tabs.iter().position(|t| t.path == path) else {
            return;
        };
        match save {
            Some(true) => {
                let saved = match &self.tabs[at].content {
                    Content::Text(editor, _) => editor.update(cx, |e, cx| e.save(cx)).is_ok(),
                    _ => true,
                };
                if saved {
                    self.remove(at);
                }
            }
            Some(false) => self.remove(at),
            None => {}
        }
        cx.notify();
    }

    fn remove(&mut self, at: usize) {
        self.tabs.remove(at);
        if self.active >= self.tabs.len() || self.active > at {
            self.active = self.active.saturating_sub(1);
        }
    }

    fn header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let paths: Vec<&Path> = self.tabs.iter().map(|t| t.path.as_path()).collect();
        let mut tabs = h_flex()
            .id("editor-tabs")
            .flex_1()
            .min_w_0()
            .overflow_x_scroll();
        for (i, tab) in self.tabs.iter().enumerate() {
            let path = tab.path.clone();
            let close_path = tab.path.clone();
            let dirty = is_dirty(tab, cx);
            tabs = tabs.child(
                h_flex()
                    .id(SharedString::from(format!("editor-tab-{i}")))
                    .flex_none()
                    .gap_1()
                    .pl_2()
                    .pr_0p5()
                    .h(px(24.))
                    .rounded(px(6.))
                    .when(i == self.active, |t| {
                        t.bg(cx.theme().colors().element_selected)
                    })
                    .tooltip(Tooltip::text(tab.path.display().to_string()))
                    .on_click(cx.listener(move |this, _, window, cx| {
                        this.open(&path, None, window, cx);
                    }))
                    .child(
                        Label::new(doc::tab_label(&tab.path, &paths))
                            .size(LabelSize::Small)
                            .single_line(),
                    )
                    .when(dirty, |t| {
                        t.child(
                            div()
                                .size(px(6.))
                                .rounded_full()
                                .bg(Color::Modified.color(cx)),
                        )
                    })
                    .child(
                        IconButton::new(
                            SharedString::from(format!("editor-close-{i}")),
                            IconName::Close,
                        )
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Close"))
                        .on_click(cx.listener(move |this, _, _, cx| this.close(&close_path, cx))),
                    ),
            );
        }
        let active = self.active_path().map(Path::to_path_buf);
        let line = self.active_editor().map(|e| {
            let state = &e.read(cx).state;
            state.buffer().row_of(state.primary().head) as u32 + 1
        });
        let dirty = self.tabs.get(self.active).is_some_and(|t| is_dirty(t, cx));
        let (left, right) = self.can_move;
        let weak = cx.weak_entity();
        ferrum_ui::title_row("editor-header", false, window)
            .gap_1()
            .pl_2()
            .child(tabs)
            .when(dirty, |row| {
                row.child(
                    ferrum_ui::title_button("editor-save", IconName::Check, "Save (⌘S)")
                        .on_click(cx.listener(|this, _, _, cx| this.save_active(cx))),
                )
            })
            .when_some(active, |row, path| {
                row.child(
                    ferrum_ui::title_button(
                        "editor-open-in-ide",
                        IconName::ArrowUpRight,
                        "Open in FerrumIDE",
                    )
                    .on_click(cx.listener(move |_, _, _, cx| {
                        cx.emit(EditorEvent::OpenInIde {
                            path: path.clone(),
                            line,
                        });
                    })),
                )
            })
            .child(
                PopoverMenu::new("editor-menu")
                    .anchor(gpui::Anchor::TopRight)
                    .trigger(ferrum_ui::title_button(
                        "editor-menu-button",
                        IconName::Ellipsis,
                        "Editor",
                    ))
                    .menu(move |window, cx| {
                        let weak = weak.clone();
                        Some(ContextMenu::build(window, cx, move |menu, _, _| {
                            let emit = |event: EditorEvent| {
                                let weak = weak.clone();
                                move |_: &mut Window, cx: &mut App| {
                                    weak.update(cx, |_, cx| cx.emit(event.clone())).ok();
                                }
                            };
                            let menu = if left {
                                menu.entry("Move Left", None, emit(EditorEvent::Move(-1)))
                            } else {
                                menu.disabled_entry("Move Left")
                            };
                            let menu = if right {
                                menu.entry("Move Right", None, emit(EditorEvent::Move(1)))
                            } else {
                                menu.disabled_entry("Move Right")
                            };
                            menu.separator()
                                .entry("Close Editor", None, emit(EditorEvent::Close))
                        }))
                    }),
            )
    }

    fn confirm_bar(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let path = self.confirm_close.as_ref()?;
        let name = path.file_name().map_or_else(
            || path.display().to_string(),
            |n| n.to_string_lossy().into(),
        );
        Some(
            h_flex()
                .flex_none()
                .gap_2()
                .px_3()
                .py_1p5()
                .border_b_1()
                .border_color(cx.theme().colors().border)
                .child(
                    Label::new(format!("Save the changes to {name}?"))
                        .size(LabelSize::Small)
                        .single_line(),
                )
                .child(div().flex_1())
                .child(
                    Button::new("editor-confirm-cancel", "Cancel")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .on_click(cx.listener(|this, _, _, cx| this.answer_close(None, cx))),
                )
                .child(
                    Button::new("editor-confirm-discard", "Discard")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .on_click(cx.listener(|this, _, _, cx| this.answer_close(Some(false), cx))),
                )
                .child(
                    Button::new("editor-confirm-save", "Save")
                        .style(ButtonStyle::Filled)
                        .size(ButtonSize::Compact)
                        .on_click(cx.listener(|this, _, _, cx| this.answer_close(Some(true), cx))),
                )
                .into_any_element(),
        )
    }

    fn body(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(tab) = self.tabs.get(self.active) else {
            return v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .child(empty_state(
                    "No file open",
                    "Open a file from the Files pane, the chat or git.",
                ))
                .into_any_element();
        };
        match &tab.content {
            Content::Text(editor, _) => editor.clone().into_any_element(),
            Content::Image => div()
                .size_full()
                .p_4()
                .flex()
                .items_center()
                .justify_center()
                .bg(cx.theme().colors().editor_background)
                .child(
                    img(tab.path.clone())
                        .max_w_full()
                        .max_h_full()
                        .object_fit(ObjectFit::Contain),
                )
                .into_any_element(),
            Content::Other { reason } => v_flex()
                .size_full()
                .items_center()
                .justify_center()
                .gap_2()
                .p_4()
                .child(empty_state(
                    tab.path
                        .file_name()
                        .map_or_else(String::new, |n| n.to_string_lossy().into()),
                    reason.clone(),
                ))
                .into_any_element(),
        }
    }
}

fn is_dirty(tab: &Tab, cx: &App) -> bool {
    match &tab.content {
        Content::Text(editor, _) => editor.read(cx).is_dirty(),
        _ => false,
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        match self.active_editor() {
            Some(editor) => editor.focus_handle(cx),
            None => self.focus_handle.clone(),
        }
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        v_flex()
            .id("vibe-editor")
            .track_focus(&self.focus_handle)
            .size_full()
            .min_w_0()
            .child(self.header(window, cx))
            .children(self.confirm_bar(cx))
            .child(div().flex_1().min_h_0().child(self.body(cx)))
    }
}

//! A code editor for working-tree files in the stand-alone app: FerrumIDE's
//! editing core (`ferrumeditor-core`: buffer, multi-cursor commands, undo,
//! tree-sitter highlighting, find) under a lighter view without language
//! servers. Rendering lives in `element`, `decorations` and `layout`, mouse
//! and IME input in `input`, key commands in `commands`, the find bar in
//! `find`; `pane` shows it in the detail card.

mod commands;
mod decorations;
mod element;
mod find;
mod input;
mod layout;
pub mod pane;

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ferrum_ui::theme::ActiveTheme as _;
use ferrumeditor_core::config::Config;
use ferrumeditor_core::display::DisplayMap;
use ferrumeditor_core::editor::EditorState;
use ferrumeditor_core::files::{DiskStamp, LineEnding, LoadedFile, SaveOptions};
use ferrumeditor_core::highlight::{self, HighlightSpan};
use ferrumeditor_core::language::Language;
use ferrumeditor_core::theme::Theme;
use gpui::{
    App, Context, CursorStyle, EventEmitter, FocusHandle, Focusable, MouseButton, Pixels, Render,
    Task, Window, div, prelude::*, px,
};

pub use commands::{EDITOR_CONTEXT, PANE_CONTEXT, bind_keys};
use layout::LayoutSnapshot;

/// Files above this size are not syntax highlighted.
const HIGHLIGHT_LIMIT: usize = 4 * 1024 * 1024;
const HIGHLIGHT_DEBOUNCE: Duration = Duration::from_millis(20);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CodeEditorEvent {
    /// The text changed (the dirty state may have changed).
    Edited,
    /// The buffer was written to disk.
    Saved,
    /// Saving failed.
    Error(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Char,
    Word,
    Line,
}

#[derive(Debug, Clone)]
struct DragState {
    mode: DragMode,
    /// Range selected by the initial click (word / line), or the anchor.
    origin: Range<usize>,
}

pub struct CodeEditor {
    pub state: EditorState,
    path: PathBuf,
    line_ending: LineEnding,
    bom: bool,
    disk_stamp: Option<DiskStamp>,
    focus_handle: FocusHandle,
    /// Unwrapped rows, for the core's vertical motions.
    display_map: DisplayMap,
    /// Syntax, selection and cursor colours (FerrumIDE's built-in theme
    /// matching the window's appearance).
    theme: Arc<Theme>,
    scroll_y: Pixels,
    scroll_x: Pixels,
    autoscroll: bool,
    /// 0-based line to show a third of the way down once laid out.
    pending_line: Option<usize>,
    highlights: Arc<Vec<HighlightSpan>>,
    highlight_version: Option<u64>,
    highlight_cancel: Arc<AtomicUsize>,
    highlight_task: Option<Task<()>>,
    find: find::FindBar,
    marked_range: Option<Range<usize>>,
    drag: Option<DragState>,
    last_layout: Option<LayoutSnapshot>,
    /// Widest line in cells (for horizontal scrolling), keyed by version.
    widest_line: (u64, usize),
}

impl EventEmitter<CodeEditorEvent> for CodeEditor {}

impl CodeEditor {
    /// Reads `path` (refusing binary, non-UTF-8 and huge files).
    pub fn load(path: &Path) -> Result<LoadedFile, String> {
        ferrumeditor_core::files::load(path)
            .map_err(|e| format!("Cannot edit {}: {e}", path.display()))
    }

    pub fn new(file: LoadedFile, path: PathBuf, cx: &mut Context<Self>) -> Self {
        let first_line = file.text.lines().next().unwrap_or("");
        let language = Language::detect(&path, first_line);
        let settings = Config::default().edit_settings(language);
        let tab_size = settings.tab_size;
        let state = EditorState::new(&file.text, language, settings);
        let mut this = Self {
            state,
            disk_stamp: ferrumeditor_core::files::disk_stamp(&path),
            path,
            line_ending: file.line_ending,
            bom: file.bom,
            focus_handle: cx.focus_handle(),
            display_map: DisplayMap::new(tab_size, None),
            theme: Arc::new(Theme::dark()),
            scroll_y: px(0.),
            scroll_x: px(0.),
            autoscroll: false,
            pending_line: None,
            highlights: Arc::default(),
            highlight_version: None,
            highlight_cancel: Arc::default(),
            highlight_task: None,
            find: find::FindBar::default(),
            marked_range: None,
            drag: None,
            last_layout: None,
            widest_line: (u64::MAX, 0),
        };
        this.schedule_highlight(cx);
        this
    }

    #[must_use]
    pub fn is_dirty(&self) -> bool {
        self.state.is_dirty()
    }

    /// The file being edited.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    /// Puts the cursor at the start of 1-based `line` and scrolls to it.
    pub fn go_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        let row = line
            .saturating_sub(1)
            .min(self.state.buffer().line_count() - 1);
        self.state.go_to_point(row, 0);
        self.pending_line = Some(row);
        cx.notify();
    }

    /// Writes the buffer to its file, keeping its line endings and BOM.
    pub fn save(&mut self, cx: &mut Context<Self>) -> Result<(), String> {
        let options = SaveOptions {
            line_ending: self.line_ending,
            bom: self.bom,
            ..SaveOptions::default()
        };
        let bytes = ferrumeditor_core::files::encode(&self.state.text(), options);
        if let Err(e) = ferrumeditor_core::files::write_atomic(&self.path, &bytes) {
            let message = format!("Could not save {}: {e}", self.path.display());
            cx.emit(CodeEditorEvent::Error(message.clone()));
            return Err(message);
        }
        self.state.mark_saved();
        self.disk_stamp = ferrumeditor_core::files::disk_stamp(&self.path);
        cx.emit(CodeEditorEvent::Saved);
        cx.notify();
        Ok(())
    }

    /// Re-reads the file when it changed on disk (a discard, a checkout)
    /// and the buffer has no edits of its own. True if it was reloaded.
    pub fn reload_if_changed(&mut self, cx: &mut Context<Self>) -> bool {
        let stamp = ferrumeditor_core::files::disk_stamp(&self.path);
        if self.is_dirty() || stamp.is_none() || stamp == self.disk_stamp {
            return false;
        }
        let Ok(file) = ferrumeditor_core::files::load(&self.path) else {
            return false;
        };
        self.line_ending = file.line_ending;
        self.bom = file.bom;
        self.state.set_text_preserving(&file.text);
        self.state.mark_saved();
        self.disk_stamp = stamp;
        self.after_edit(cx);
        true
    }

    /// Called after every command that may have edited the buffer.
    fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.schedule_highlight(cx);
        self.find.refresh(&self.state);
        cx.emit(CodeEditorEvent::Edited);
        cx.notify();
    }

    fn schedule_highlight(&mut self, cx: &mut Context<Self>) {
        let version = self.state.buffer().version();
        if self.highlight_version == Some(version) {
            return;
        }
        let language = self.state.language;
        if !highlight::supports(language) || self.state.buffer().len() > HIGHLIGHT_LIMIT {
            self.highlights = Arc::default();
            self.highlight_version = Some(version);
            self.highlight_task = None;
            return;
        }
        self.highlight_cancel.store(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicUsize::new(0));
        self.highlight_cancel = cancel.clone();
        let text = self.state.text();
        let first = self.highlight_version.is_none();
        self.highlight_task = Some(cx.spawn(async move |this, cx| {
            if !first {
                cx.background_executor().timer(HIGHLIGHT_DEBOUNCE).await;
            }
            let flag = cancel.clone();
            let spans = cx
                .background_executor()
                .spawn(async move { highlight::highlight(language, &text, Some(&flag)) })
                .await;
            if cancel.load(Ordering::Relaxed) != 0 {
                return;
            }
            this.update(cx, |this, cx| {
                this.highlights = Arc::new(spans);
                this.highlight_version = Some(version);
                cx.notify();
            })
            .ok();
        }));
    }

    /// The theme for the window's appearance (rebuilt when it flips).
    fn sync_theme(&mut self, cx: &App) {
        let dark = !cx.theme().appearance.is_light();
        if self.theme.dark != dark {
            self.theme = Arc::new(Theme::base(if dark { "dark" } else { "light" }, dark));
        }
    }
}

impl Focusable for CodeEditor {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for CodeEditor {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_theme(cx);
        let find_bar = self.render_find_bar(window, cx);
        gpui::div()
            .key_context(PANE_CONTEXT)
            .size_full()
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .on_action(cx.listener(Self::on_command))
            .children(find_bar)
            .child(
                div()
                    .id("code-editor")
                    .key_context(EDITOR_CONTEXT)
                    .track_focus(&self.focus_handle)
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .min_h_0()
                    .bg(cx.theme().colors().editor_background)
                    .cursor(CursorStyle::IBeam)
                    .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
                    .on_mouse_move(cx.listener(Self::on_mouse_move))
                    .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
                    .on_scroll(cx.listener(Self::on_scroll))
                    .child(element::EditorElement::new(cx.entity())),
            )
    }
}

#[cfg(test)]
impl CodeEditor {
    pub fn text(&self) -> String {
        self.state.text()
    }

    /// 1-based line of the primary cursor.
    pub fn cursor_line(&self) -> usize {
        self.state.buffer().row_of(self.state.primary().head) + 1
    }
}

//! The text editor view: an [`EditorState`] from the core crate plus file
//! metadata, scroll state, syntax highlights, git marks and search
//! matches. Rendering lives in `element`, input in `input`, command
//! dispatch in `commands`, background work in `background`, find support
//! in `search`; language server features in `lsp`, `lsp_actions`,
//! `completion` and `popups`.

mod background;
mod commands;
mod completion;
mod decorations;
mod element;
mod input;
mod layout;
mod lsp;
mod lsp_actions;
mod popups;
mod search;

use ferrum_ui::theme::ActiveTheme as _;
use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicUsize;

use ferrumeditor_core::display::DisplayMap;
use ferrumeditor_core::editor::EditorState;
use ferrumeditor_core::files::{DiskStamp, LineEnding, LoadedFile, SaveOptions};
use ferrumeditor_core::git::LineMark;
use ferrumeditor_core::highlight::HighlightSpan;
use ferrumeditor_core::language::Language;
use gpui::{
    App, Context, CursorStyle, EventEmitter, FocusHandle, Focusable, IntoElement, MouseButton,
    Pixels, Render, SharedString, Subscription, Task, Window, div, prelude::*, px,
};

pub use layout::LayoutSnapshot;
pub use search::SearchState;

use crate::lsp_store::LspStore;
use crate::settings::Settings;

/// Files above this size are not syntax highlighted.
const HIGHLIGHT_LIMIT: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EditorEvent {
    /// Text changed (dirty state / title may have changed).
    Edited,
    /// Cursors or selections moved.
    SelectionsChanged,
    Saved,
    Focused,
    /// Go to definition found these places.
    Navigate {
        locations: Vec<ferrumeditor_core::lsp::Location>,
        encoding: ferrumeditor_core::lsp::PositionEncoding,
    },
    /// A message for the status notice.
    Notice(String),
    /// Apply a workspace edit (rename, code action) across files, then run
    /// the server `command` if there is one.
    ApplyEdit {
        edit: ferrumeditor_core::lsp::WorkspaceEdit,
        encoding: ferrumeditor_core::lsp::PositionEncoding,
        label: String,
        command: Option<ferrumeditor_core::lsp::features::LspCommand>,
    },
    /// Find all references found these places.
    References {
        locations: Vec<ferrumeditor_core::lsp::Location>,
        encoding: ferrumeditor_core::lsp::PositionEncoding,
        symbol: String,
    },
    /// Code actions to pick from.
    CodeActions(Vec<ferrumeditor_core::lsp::features::CodeAction>),
    /// The breakpoint lane of the gutter was clicked at this 0-based line.
    ToggleBreakpoint(usize),
}

/// Debugger marks for the gutter: breakpoints (0-based line, verified by
/// the adapter: `None` = no session) and where execution is paused.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DebugMarks {
    pub breakpoints: Vec<(usize, Option<bool>)>,
    pub exec_line: Option<usize>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum DragMode {
    Char,
    Word,
    Line,
    Column,
}

#[derive(Debug, Clone)]
struct DragState {
    mode: DragMode,
    /// Range selected by the initial click (word / line), or the anchor.
    origin: Range<usize>,
    column_origin: Option<ferrumeditor_core::display::DisplayPoint>,
}

pub struct EditorView {
    pub state: EditorState,
    pub path: Option<PathBuf>,
    pub line_ending: LineEnding,
    pub bom: bool,
    pub disk_stamp: Option<DiskStamp>,
    focus_handle: FocusHandle,
    pub display_map: DisplayMap,
    pub soft_wrap: bool,
    pub show_line_numbers: bool,
    pub show_indent_guides: bool,
    scroll_y: Pixels,
    scroll_x: Pixels,
    autoscroll: bool,
    highlights: Arc<Vec<HighlightSpan>>,
    highlight_version: Option<u64>,
    highlight_cancel: Arc<AtomicUsize>,
    highlight_task: Option<Task<()>>,
    pub git_repo: Option<PathBuf>,
    git_base: Option<Arc<String>>,
    git_marks: Arc<Vec<(usize, LineMark)>>,
    git_version: Option<u64>,
    git_task: Option<Task<()>>,
    pub search: SearchState,
    marked_range: Option<Range<usize>>,
    drag: Option<DragState>,
    last_layout: Option<LayoutSnapshot>,
    /// Widest line in chars (for horizontal scrolling), keyed by version.
    widest_line: (u64, usize),
    pub(crate) lsp: lsp::LspState,
    actions: lsp_actions::LspActions,
    completion: Option<completion::CompletionMenu>,
    completion_task: Option<Task<()>>,
    pub debug_marks: DebugMarks,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<EditorEvent> for EditorView {}

impl EditorView {
    pub fn new(
        text: &str,
        path: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let focus_handle = cx.focus_handle();
        let focus = cx.on_focus(&focus_handle, window, |_, _, cx| {
            cx.emit(EditorEvent::Focused)
        });
        let mut subscriptions = vec![focus];
        if let Some(store) = LspStore::global(cx) {
            subscriptions
                .push(cx.subscribe(&store, |this, _, event, cx| this.on_lsp_event(event, cx)));
        }
        subscriptions.push(cx.on_release(|this, _| {
            if let Some(doc) = this.lsp.doc.take() {
                doc.client.notify(
                    "textDocument/didClose",
                    serde_json::json!({ "textDocument": { "uri": doc.uri } }),
                );
            }
        }));
        let first_line = text.lines().next().unwrap_or("");
        let language = path
            .as_deref()
            .map_or(Language::PlainText, |p| Language::detect(p, first_line));
        let settings = cx.global::<Settings>();
        let config = &settings.config;
        let state = EditorState::new(text, language, config.edit_settings(language));
        let git_repo = path.as_deref().and_then(ferrumeditor_core::git::repo_root);
        let mut this = Self {
            state,
            path,
            line_ending: LineEnding::Lf,
            bom: false,
            disk_stamp: None,
            focus_handle,
            display_map: DisplayMap::new(config.tab_size, None),
            soft_wrap: config.soft_wrap_for(language),
            show_line_numbers: config.show_line_numbers,
            show_indent_guides: config.show_indent_guides,
            scroll_y: px(0.),
            scroll_x: px(0.),
            autoscroll: false,
            highlights: Arc::default(),
            highlight_version: None,
            highlight_cancel: Arc::default(),
            highlight_task: None,
            git_repo,
            git_base: None,
            git_marks: Arc::default(),
            git_version: None,
            git_task: None,
            search: SearchState::default(),
            marked_range: None,
            drag: None,
            last_layout: None,
            widest_line: (u64::MAX, 0),
            lsp: lsp::LspState::default(),
            actions: lsp_actions::LspActions::default(),
            completion: None,
            completion_task: None,
            debug_marks: DebugMarks::default(),
            _subscriptions: subscriptions,
        };
        this.refresh_disk_stamp();
        this.schedule_highlight(cx);
        this.load_git_base(cx);
        this.attach_lsp(cx);
        this
    }

    /// Opens a loaded file.
    pub fn from_file(
        file: LoadedFile,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self::new(&file.text, Some(path), window, cx);
        this.line_ending = file.line_ending;
        this.bom = file.bom;
        this
    }

    pub fn language(&self) -> Language {
        self.state.language
    }

    pub fn set_language(&mut self, language: Language, cx: &mut Context<Self>) {
        let changed = self.state.language != language;
        self.state.language = language;
        self.apply_settings(cx);
        self.highlight_version = None;
        self.schedule_highlight(cx);
        if changed {
            self.attach_lsp(cx);
        }
    }

    /// Re-reads per-language settings after config.toml changed.
    pub fn apply_settings(&mut self, cx: &mut Context<Self>) {
        let settings = cx.global::<Settings>();
        self.state.settings = settings.config.edit_settings(self.state.language);
        self.soft_wrap = settings.config.soft_wrap_for(self.state.language);
        self.show_line_numbers = settings.config.show_line_numbers;
        self.show_indent_guides = settings.config.show_indent_guides;
        cx.notify();
    }

    pub fn title(&self) -> SharedString {
        self.path.as_deref().and_then(Path::file_name).map_or_else(
            || "untitled".into(),
            |n| n.to_string_lossy().to_string().into(),
        )
    }

    pub fn is_dirty(&self) -> bool {
        self.state.is_dirty() || (self.path.is_none() && !self.state.buffer().is_empty())
    }

    pub fn focus(&self, window: &mut Window, cx: &mut App) {
        window.focus(&self.focus_handle, cx);
    }

    pub fn save_options(&self, cx: &App) -> SaveOptions {
        let config = &cx.global::<Settings>().config;
        SaveOptions {
            line_ending: self.line_ending,
            bom: self.bom,
            trim_trailing_whitespace: config.trim_trailing_whitespace,
            ensure_final_newline: config.ensure_final_newline,
        }
    }

    /// Writes the buffer to `path` (or its current path).
    pub fn save(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let path = path
            .or_else(|| self.path.clone())
            .ok_or_else(|| anyhow::anyhow!("no path"))?;
        let bytes = ferrumeditor_core::files::encode(&self.state.text(), self.save_options(cx));
        ferrumeditor_core::files::write_atomic(&path, &bytes)?;
        let renamed = self.path.as_deref() != Some(path.as_path());
        if renamed {
            self.detach_lsp(cx);
        }
        self.path = Some(path);
        if renamed {
            let language = self.path.as_deref().map_or(Language::PlainText, |p| {
                Language::detect(p, &self.state.buffer().line(0))
            });
            self.git_repo = self
                .path
                .as_deref()
                .and_then(ferrumeditor_core::git::repo_root);
            self.set_language(language, cx);
            self.attach_lsp(cx);
        }
        // Applying trim / final newline to the buffer keeps it equal to disk.
        let saved = String::from_utf8_lossy(&bytes).replace("\r\n", "\n");
        let saved = saved.strip_prefix('\u{feff}').unwrap_or(&saved).to_string();
        if saved != self.state.text() {
            self.state.set_text_preserving(&saved);
            self.after_edit(cx);
        }
        self.state.mark_saved();
        self.refresh_disk_stamp();
        self.load_git_base(cx);
        self.lsp_did_save();
        cx.emit(EditorEvent::Saved);
        cx.notify();
        Ok(())
    }

    /// The file was moved on disk (a server's rename-file edit).
    pub fn moved_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        self.detach_lsp(cx);
        let language = Language::detect(&path, &self.state.buffer().line(0));
        self.git_repo = ferrumeditor_core::git::repo_root(&path);
        self.path = Some(path);
        self.refresh_disk_stamp();
        let same_language = self.state.language == language;
        // A new language attaches by itself.
        self.set_language(language, cx);
        if same_language {
            self.attach_lsp(cx);
        }
        cx.emit(EditorEvent::Edited);
    }

    pub fn refresh_disk_stamp(&mut self) {
        self.disk_stamp = self
            .path
            .as_deref()
            .and_then(ferrumeditor_core::files::disk_stamp);
    }

    /// Replaces the buffer with the file's current contents (undoable).
    pub fn reload_from_disk(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        let Some(path) = self.path.clone() else {
            return Ok(());
        };
        let file = ferrumeditor_core::files::load(&path)?;
        self.line_ending = file.line_ending;
        self.bom = file.bom;
        self.state.set_text_preserving(&file.text);
        self.state.mark_saved();
        self.refresh_disk_stamp();
        self.after_edit(cx);
        self.load_git_base(cx);
        Ok(())
    }

    /// Called after every command that may have edited the buffer.
    pub(crate) fn after_edit(&mut self, cx: &mut Context<Self>) {
        self.schedule_highlight(cx);
        self.schedule_git_diff(cx);
        self.schedule_lsp_sync(cx);
        self.hide_hover(cx);
        self.search.refresh(&self.state);
        cx.emit(EditorEvent::Edited);
        cx.notify();
    }

    pub(crate) fn request_autoscroll(&mut self) {
        self.autoscroll = true;
    }

    /// Scrolls so `line` (0-based) is near the top third of the view.
    pub fn scroll_to_line(&mut self, line: usize, cx: &mut Context<Self>) {
        self.autoscroll = true;
        let offset = self
            .state
            .buffer()
            .point_to_offset(ferrumeditor_core::buffer::Point::new(line, 0));
        let row = self.display_map.to_display(self.state.buffer(), offset).row;
        if let Some(layout) = &self.last_layout {
            let target = layout.line_height * row as f32 - layout.bounds.size.height / 3.;
            self.scroll_y = target.max(px(0.));
            self.autoscroll = false;
        }
        cx.notify();
    }

    pub fn scroll_line(&self) -> usize {
        self.last_layout.as_ref().map_or(0, |l| {
            let (row, _) = self
                .display_map
                .buffer_segment(self.state.buffer(), l.first_row);
            row
        })
    }

    /// Restores a scroll position (first visible buffer line).
    pub fn set_scroll_line(&mut self, line: usize) {
        let row = self.display_map.first_display_row(line);
        let line_height = self.last_layout.as_ref().map_or(px(20.), |l| l.line_height);
        self.scroll_y = line_height * row as f32;
    }

    pub fn set_debug_marks(&mut self, marks: DebugMarks, cx: &mut Context<Self>) {
        if self.debug_marks != marks {
            self.debug_marks = marks;
            cx.notify();
        }
    }

    pub fn cursor_point(&self) -> ferrumeditor_core::buffer::Point {
        self.state
            .buffer()
            .offset_to_point(self.state.primary().head)
    }
}

#[cfg(test)]
impl EditorView {
    pub fn completion_labels(&self) -> Option<Vec<String>> {
        self.completion.as_ref().map(|m| {
            m.matches
                .iter()
                .map(|i| m.entries[*i].label.clone())
                .collect()
        })
    }

    pub fn hover_popup(&self) -> Option<&lsp::HoverPopup> {
        self.lsp.hover.as_ref()
    }

    pub fn server_ready(&self) -> bool {
        self.lsp.doc.as_ref().is_some_and(|d| d.client.is_ready())
    }

    pub fn rename_open(&self) -> bool {
        self.actions.rename.is_some()
    }

    pub fn signature_label(&self) -> Option<String> {
        self.actions
            .signature
            .as_ref()
            .map(|s| s.help.label.clone())
    }
}

impl Focusable for EditorView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for EditorView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let popups = self.render_popups(window, cx);
        div()
            .id("editor")
            .key_context("FerrumEditor")
            .track_focus(&self.focus_handle)
            .size_full()
            .min_w_0()
            .min_h_0()
            .bg(cx.theme().colors().editor_background)
            .cursor(CursorStyle::IBeam)
            .on_action(cx.listener(Self::on_run_command))
            .on_mouse_down(MouseButton::Left, cx.listener(Self::on_mouse_down))
            .on_mouse_move(cx.listener(Self::on_mouse_move))
            .on_mouse_up(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_mouse_up_out(MouseButton::Left, cx.listener(Self::on_mouse_up))
            .on_scroll(cx.listener(Self::on_scroll))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if !hovered {
                    this.hover_left(cx);
                }
            }))
            .child(element::EditorElement::new(cx.entity()))
            .children(popups)
    }
}

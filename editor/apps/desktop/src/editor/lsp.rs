//! The editor's side of language servers: opening the document on its
//! server, sending edits, turning published diagnostics into buffer
//! ranges, hover popups, problem navigation and go to definition.

use std::ops::Range;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use ferrumeditor_core::completion::is_word_char;
use ferrumeditor_core::lsp::position::{offset_to_position, position_to_offset, range_to_offsets};
use ferrumeditor_core::lsp::types::{parse_hover, parse_locations};
use ferrumeditor_core::lsp::{Client, Position, PositionEncoding, Severity, path_to_uri, servers};
use gpui::{Context, Task};
use serde_json::{Value, json};

use super::{EditorEvent, EditorView};
use crate::lsp_store::{Attachment, LspEvent, LspStore, ServerStatus};
use crate::settings::Settings;

/// Edits are sent to the server after this pause (and before requests).
const SYNC_DEBOUNCE: Duration = Duration::from_millis(120);
const HOVER_DELAY: Duration = Duration::from_millis(400);
/// Time to move the mouse from the text onto the popup.
const HOVER_GRACE: Duration = Duration::from_millis(200);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditorDiagnostic {
    pub range: Range<usize>,
    pub severity: Severity,
    pub message: String,
}

pub(super) struct LspDoc {
    pub server_id: u64,
    pub client: Client,
    pub uri: String,
    version: i32,
    /// Buffer version the server has seen.
    synced: u64,
}

/// What the hover popup shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverPopup {
    /// Moving the mouse outside this range closes the popup.
    pub range: Range<usize>,
    /// Buffer offset the popup is drawn above.
    pub anchor: usize,
    pub diagnostics: Vec<(Severity, String)>,
    pub text: Option<String>,
}

#[derive(Default)]
pub struct LspState {
    pub(super) doc: Option<LspDoc>,
    pub diagnostics: Arc<Vec<EditorDiagnostic>>,
    pub(super) hover: Option<HoverPopup>,
    /// The mouse is over the hover popup (keeps it open).
    pub(super) pointer_in_hover: bool,
    hide_task: Option<Task<()>>,
    attach_task: Option<Task<()>>,
    sync_task: Option<Task<()>>,
    hover_task: Option<Task<()>>,
    definition_task: Option<Task<()>>,
}

impl EditorView {
    /// Opens this file on its language server (if there is one).
    pub(crate) fn attach_lsp(&mut self, cx: &mut Context<Self>) {
        self.detach_lsp(cx);
        let (Some(path), Some(store)) = (self.path.clone(), LspStore::global(cx)) else {
            return;
        };
        let language = self.state.language;
        let task = store.update(cx, |store, cx| store.attach(path.clone(), language, cx));
        self.lsp.attach_task = Some(cx.spawn(async move |this, cx| {
            if let Some(attachment) = task.await {
                this.update(cx, |this, cx| this.open_on_server(attachment, path, cx))
                    .ok();
            }
        }));
    }

    fn open_on_server(&mut self, attachment: Attachment, path: PathBuf, cx: &mut Context<Self>) {
        if self.path.as_deref() != Some(path.as_path()) {
            return;
        }
        let uri = path_to_uri(&path);
        attachment.client.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": servers::language_id(self.state.language, &path),
                    "version": 1,
                    "text": self.state.text(),
                }
            }),
        );
        self.lsp.doc = Some(LspDoc {
            server_id: attachment.server_id,
            client: attachment.client,
            uri,
            version: 1,
            synced: self.state.buffer().version(),
        });
        self.refresh_diagnostics(cx);
        cx.notify();
    }

    /// Closes the document on its server.
    pub(crate) fn detach_lsp(&mut self, cx: &mut Context<Self>) {
        self.lsp.attach_task = None;
        self.lsp.sync_task = None;
        self.lsp.hover = None;
        self.completion = None;
        if let Some(doc) = self.lsp.doc.take() {
            doc.client.notify(
                "textDocument/didClose",
                json!({ "textDocument": { "uri": doc.uri } }),
            );
        }
        if !self.lsp.diagnostics.is_empty() {
            self.lsp.diagnostics = Arc::default();
            cx.notify();
        }
    }

    pub(crate) fn on_lsp_event(&mut self, event: &LspEvent, cx: &mut Context<Self>) {
        match event {
            LspEvent::Diagnostics(path) if self.path.as_ref() == Some(path) => {
                self.refresh_diagnostics(cx);
            }
            LspEvent::Restarted => {
                // The old server is gone; don't send it didClose.
                self.lsp.doc = None;
                self.attach_lsp(cx);
            }
            _ => {}
        }
    }

    pub fn server_id(&self) -> Option<u64> {
        self.lsp.doc.as_ref().map(|d| d.server_id)
    }

    pub fn has_language_server(&self) -> bool {
        self.lsp
            .doc
            .as_ref()
            .is_some_and(|d| !d.client.is_stopped())
    }

    pub(super) fn encoding(&self) -> PositionEncoding {
        self.lsp
            .doc
            .as_ref()
            .map_or(PositionEncoding::Utf16, |d| d.client.position_encoding())
    }

    /// Sends pending edits after a short pause.
    pub(super) fn schedule_lsp_sync(&mut self, cx: &mut Context<Self>) {
        if self.lsp.doc.is_none() {
            return;
        }
        self.lsp.sync_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SYNC_DEBOUNCE).await;
            this.update(cx, |this, _| this.flush_lsp_changes()).ok();
        }));
    }

    /// Sends the full text if the server hasn't seen this version.
    pub(super) fn flush_lsp_changes(&mut self) {
        let version = self.state.buffer().version();
        let Some(doc) = self.lsp.doc.as_mut() else {
            return;
        };
        if doc.synced == version {
            return;
        }
        doc.version += 1;
        doc.synced = version;
        doc.client.notify(
            "textDocument/didChange",
            json!({
                "textDocument": { "uri": doc.uri, "version": doc.version },
                "contentChanges": [{ "text": self.state.text() }],
            }),
        );
    }

    pub(super) fn lsp_did_save(&mut self) {
        self.flush_lsp_changes();
        if let Some(doc) = &self.lsp.doc {
            doc.client.notify(
                "textDocument/didSave",
                json!({ "textDocument": { "uri": doc.uri } }),
            );
        }
    }

    /// `textDocument` + `position` params for the caret or `offset`.
    pub(super) fn position_params(&self, offset: usize) -> Option<Value> {
        let doc = self.lsp.doc.as_ref()?;
        let position = offset_to_position(self.state.buffer(), offset, self.encoding());
        Some(json!({
            "textDocument": { "uri": doc.uri },
            "position": position.to_json(),
        }))
    }

    /// Re-reads this file's diagnostics from the store.
    pub(crate) fn refresh_diagnostics(&mut self, cx: &mut Context<Self>) {
        let found = self
            .path
            .as_deref()
            .zip(LspStore::global(cx))
            .and_then(|(path, store)| store.read(cx).diagnostics_for(path));
        let buffer = self.state.buffer();
        let mut list: Vec<EditorDiagnostic> = match found {
            Some((encoding, raw)) => raw
                .iter()
                .map(|d| {
                    let mut range = range_to_offsets(buffer, d.range, encoding);
                    if range.is_empty() {
                        // Point diagnostics: mark the next (or previous) char.
                        let next = buffer.next_char_offset(range.start);
                        let row_end = buffer.line_end(buffer.row_of(range.start));
                        range = if next <= row_end && next > range.start {
                            range.start..next
                        } else {
                            buffer.prev_char_offset(range.start)..range.start
                        };
                    }
                    EditorDiagnostic {
                        range,
                        severity: d.severity,
                        message: d.full_message(),
                    }
                })
                .collect(),
            None => Vec::new(),
        };
        list.sort_by_key(|d| (d.range.start, d.severity));
        self.lsp.diagnostics = Arc::new(list);
        if let Some(hover) = &self.lsp.hover
            && !hover.diagnostics.is_empty()
        {
            self.lsp.hover = None;
        }
        cx.notify();
    }

    /// Why go to definition / completion can't ask a server, for a notice.
    pub(super) fn no_server_message(&self, cx: &Context<Self>) -> String {
        let language = self.state.language;
        let config = &cx.global::<Settings>().config.lsp;
        if !config.enabled {
            return "Language servers are turned off ([lsp] enabled = false in config.toml)"
                .to_string();
        }
        let status = LspStore::global(cx)
            .and_then(|store| store.read(cx).status_for(self.server_id(), language));
        match status {
            Some(ServerStatus::Missing { language, install }) => {
                format!("No {language} language server found. Install it with: {install}")
            }
            Some(ServerStatus::Stopped { name, message }) => {
                format!("{name} stopped: {message}. Run \"Language Server: Restart\".")
            }
            Some(ServerStatus::Starting(name)) => format!("{name} is starting…"),
            _ if self.path.is_none() => "Save the file to use its language server".to_string(),
            _ if servers::server_for(language).is_none() => {
                format!("No language server for {}", language.name())
            }
            _ => "The language server is not ready yet".to_string(),
        }
    }

    // ----- Go to definition -------------------------------------------------

    pub(super) fn go_to_definition(&mut self, offset: usize, cx: &mut Context<Self>) {
        if !self.has_language_server() {
            cx.emit(EditorEvent::Notice(self.no_server_message(cx)));
            return;
        }
        self.flush_lsp_changes();
        let (Some(params), Some(doc)) = (self.position_params(offset), self.lsp.doc.as_ref())
        else {
            return;
        };
        let encoding = doc.client.position_encoding();
        let request = doc.client.request("textDocument/definition", params);
        self.lsp.definition_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |_, cx| match result {
                Ok(value) => {
                    let locations = parse_locations(&value);
                    if locations.is_empty() {
                        cx.emit(EditorEvent::Notice("No definition found".to_string()));
                    } else {
                        cx.emit(EditorEvent::Navigate {
                            locations,
                            encoding,
                        });
                    }
                }
                Err(error) => cx.emit(EditorEvent::Notice(format!(
                    "Go to definition failed: {error}"
                ))),
            })
            .ok();
        }));
    }

    /// Moves the caret to an LSP position (after a jump).
    pub fn go_to_lsp_position(
        &mut self,
        position: Position,
        encoding: PositionEncoding,
        cx: &mut Context<Self>,
    ) {
        let offset = position_to_offset(self.state.buffer(), position, encoding);
        self.state.set_cursor(offset);
        self.scroll_to_line(position.line as usize, cx);
        cx.emit(EditorEvent::SelectionsChanged);
    }

    // ----- Problems -----------------------------------------------------------

    /// Moves to the next / previous diagnostic (wrapping) and shows it.
    pub(super) fn go_to_problem(&mut self, forward: bool, cx: &mut Context<Self>) -> bool {
        let list = self.lsp.diagnostics.clone();
        if list.is_empty() {
            cx.emit(EditorEvent::Notice("No problems in this file".to_string()));
            return false;
        }
        let head = self.state.primary().head;
        let target = if forward {
            list.iter()
                .find(|d| d.range.start > head)
                .or_else(|| list.first())
        } else {
            list.iter()
                .rev()
                .find(|d| d.range.start < head)
                .or_else(|| list.last())
        };
        let Some(target) = target else {
            return false;
        };
        let start = target.range.start;
        self.state.set_cursor(start);
        self.request_autoscroll();
        self.show_hover_at(start, cx);
        true
    }

    // ----- Hover ----------------------------------------------------------------

    /// Mouse moved over the text without a button: hide a popup the mouse
    /// left, and show one after a pause.
    pub(super) fn hover_moved(&mut self, offset: Option<usize>, cx: &mut Context<Self>) {
        if let Some(hover) = &self.lsp.hover {
            if offset.is_some_and(|o| hover.range.start <= o && o <= hover.range.end) {
                return;
            }
            self.lsp.hover = None;
            cx.notify();
        }
        self.lsp.hover_task = None;
        let Some(offset) = offset else {
            return;
        };
        self.lsp.hover_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOVER_DELAY).await;
            this.update(cx, |this, cx| this.show_hover_at(offset, cx))
                .ok();
        }));
    }

    /// The mouse left the text or the popup: close soon, unless it went
    /// onto the popup.
    pub(super) fn hover_left(&mut self, cx: &mut Context<Self>) {
        if self.lsp.hover.is_none() {
            self.lsp.hover_task = None;
            return;
        }
        self.lsp.hide_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(HOVER_GRACE).await;
            this.update(cx, |this, cx| {
                if !this.lsp.pointer_in_hover {
                    this.hide_hover(cx);
                }
            })
            .ok();
        }));
    }

    pub(super) fn hide_hover(&mut self, cx: &mut Context<Self>) {
        self.lsp.hover_task = None;
        self.lsp.hide_task = None;
        self.lsp.pointer_in_hover = false;
        if self.lsp.hover.take().is_some() {
            cx.notify();
        }
    }

    /// Shows diagnostics at `offset` at once, then the server's hover text.
    pub(super) fn show_hover_at(&mut self, offset: usize, cx: &mut Context<Self>) {
        let buffer = self.state.buffer();
        let word = identifier_at(buffer, offset);
        let diagnostics: Vec<&EditorDiagnostic> = self
            .lsp
            .diagnostics
            .iter()
            .filter(|d| d.range.start <= offset && offset <= d.range.end)
            .collect();
        let mut range = word.clone();
        for d in &diagnostics {
            range = range.start.min(d.range.start)..range.end.max(d.range.end);
        }
        let popup = HoverPopup {
            anchor: if word.is_empty() {
                range.start
            } else {
                word.start
            },
            range,
            diagnostics: diagnostics
                .iter()
                .map(|d| (d.severity, d.message.clone()))
                .collect(),
            text: None,
        };
        let has_diagnostics = !popup.diagnostics.is_empty();
        self.lsp.hover = has_diagnostics.then(|| popup.clone());
        cx.notify();
        let wants_hover = self.lsp.doc.as_ref().is_some_and(|d| {
            d.client
                .capabilities()
                .is_some_and(|c| !c["hoverProvider"].is_null() && c["hoverProvider"] != false)
        });
        if word.is_empty() || !wants_hover {
            return;
        }
        self.flush_lsp_changes();
        let (Some(params), Some(doc)) = (self.position_params(offset), self.lsp.doc.as_ref())
        else {
            return;
        };
        let version = self.state.buffer().version();
        let request = doc.client.request("textDocument/hover", params);
        self.lsp.hover_task = Some(cx.spawn(async move |this, cx| {
            let Ok(value) = request.await else { return };
            let Some(text) = parse_hover(&value) else {
                return;
            };
            this.update(cx, |this, cx| {
                if this.state.buffer().version() != version {
                    return;
                }
                let mut popup = match this.lsp.hover.take() {
                    Some(current) if current.range == popup.range => current,
                    Some(_) => return,
                    None => popup,
                };
                popup.text = Some(text);
                this.lsp.hover = Some(popup);
                cx.notify();
            })
            .ok();
        }));
    }
}

/// The identifier around `offset` (empty if there is none).
pub(super) fn identifier_at(
    buffer: &ferrumeditor_core::buffer::Buffer,
    offset: usize,
) -> Range<usize> {
    let offset = buffer.clip(offset.min(buffer.len()));
    let row = buffer.row_of(offset);
    let line_start = buffer.line_start(row);
    let line = buffer.line(row);
    let col = offset - line_start;
    let before: usize = line[..col]
        .chars()
        .rev()
        .take_while(|c| is_word_char(*c))
        .map(char::len_utf8)
        .sum();
    let after: usize = line[col..]
        .chars()
        .take_while(|c| is_word_char(*c))
        .map(char::len_utf8)
        .sum();
    line_start + col - before..line_start + col + after
}

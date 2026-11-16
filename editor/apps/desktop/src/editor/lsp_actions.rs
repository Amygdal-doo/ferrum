//! Language server editing features: rename (F2, with an inline input
//! over the symbol, then a preview of the edit), find all references, code actions, formatting (the
//! command and format on save) and signature help while typing a call.
//! Multi-file results (workspace edits, reference lists, action choices)
//! go to the workspace as [`EditorEvent`]s.

use std::ops::Range;
use std::time::Duration;

use ferrum_ui::{TextInput, TextInputEvent};
use ferrumeditor_core::lsp::edit::{edits_to_offsets, parse_text_edits, parse_workspace_edit};
use ferrumeditor_core::lsp::features::{
    CodeAction, LspCommand, PrepareRename, SignatureHelp, has_capability, parse_code_actions,
    parse_prepare_rename, parse_signature_help,
};
use ferrumeditor_core::lsp::position::{offset_to_position, range_to_offsets};
use ferrumeditor_core::lsp::types::parse_locations;
use ferrumeditor_core::lsp::{PositionEncoding, TextEdit};
use gpui::{AppContext as _, Context, Entity, Focusable as _, Subscription, Task, Window};
use serde_json::{Value, json};

use super::lsp::identifier_at;
use super::{EditorEvent, EditorView};

/// Longest wait for the server's formatting before saving anyway.
const FORMAT_TIMEOUT: Duration = Duration::from_millis(1500);

/// The inline rename input over the symbol.
pub struct RenameInput {
    /// The symbol being renamed (buffer offsets).
    pub range: Range<usize>,
    pub input: Entity<TextInput>,
    original: String,
    _subscription: Subscription,
}

/// The signature help popup above the caret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignaturePopup {
    pub help: SignatureHelp,
    /// Buffer offset the popup is drawn above.
    pub anchor: usize,
}

/// Rename / references / code action / format / signature state.
#[derive(Default)]
pub struct LspActions {
    pub rename: Option<RenameInput>,
    pub signature: Option<SignaturePopup>,
    rename_task: Option<Task<()>>,
    references_task: Option<Task<()>>,
    action_task: Option<Task<()>>,
    format_task: Option<Task<()>>,
    signature_task: Option<Task<()>>,
}

impl EditorView {
    fn notice(&self, message: impl Into<String>, cx: &mut Context<Self>) {
        cx.emit(EditorEvent::Notice(message.into()));
    }

    /// The server's capabilities (empty without a server).
    fn capabilities(&self) -> Value {
        self.lsp
            .doc
            .as_ref()
            .and_then(|d| d.client.capabilities())
            .unwrap_or(Value::Null)
    }

    fn server_name(&self) -> String {
        self.lsp.doc.as_ref().map_or_else(
            || "The language server".into(),
            |d| d.client.name().to_string(),
        )
    }

    /// Checks that a server is attached and offers `capability`; explains
    /// in a notice when not.
    fn ready_for(&mut self, capability: &str, what: &str, cx: &mut Context<Self>) -> bool {
        if !self.has_language_server() {
            let message = self.no_server_message(cx);
            self.notice(message, cx);
            return false;
        }
        let ready = self.lsp.doc.as_ref().is_some_and(|d| d.client.is_ready());
        if !ready {
            let message = format!("{} is starting…", self.server_name());
            self.notice(message, cx);
            return false;
        }
        if !has_capability(&self.capabilities(), capability) {
            let message = format!("{} can't {what}", self.server_name());
            self.notice(message, cx);
            return false;
        }
        self.flush_lsp_changes();
        true
    }

    /// Applies text edits from the server as one undo step.
    pub fn apply_lsp_edits(
        &mut self,
        edits: &[TextEdit],
        encoding: PositionEncoding,
        cx: &mut Context<Self>,
    ) -> Result<usize, String> {
        let ranges = edits_to_offsets(self.state.buffer(), edits, encoding)?;
        let count = ranges.len();
        if count > 0 {
            self.state.replace_ranges(ranges);
            self.after_edit(cx);
        }
        Ok(count)
    }

    // ----- Rename ---------------------------------------------------------------

    /// F2: asks the server what can be renamed here, then shows the input.
    pub(super) fn start_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.ready_for("renameProvider", "rename symbols", cx) {
            return;
        }
        let head = self.state.primary().head;
        let word = identifier_at(self.state.buffer(), head);
        let prepare = self.capabilities()["renameProvider"]["prepareProvider"] == true;
        let (Some(params), Some(doc)) = (self.position_params(head), self.lsp.doc.as_ref()) else {
            return;
        };
        if !prepare {
            if word.is_empty() {
                self.notice("Put the caret on a symbol to rename it", cx);
            } else {
                self.open_rename_input(word, None, window, cx);
            }
            return;
        }
        let encoding = doc.client.position_encoding();
        let version = self.state.buffer().version();
        let request = doc.client.request("textDocument/prepareRename", params);
        self.actions.rename_task = Some(cx.spawn_in(window, async move |this, cx| {
            let result = request.await;
            this.update_in(cx, |this, window, cx| {
                if this.state.buffer().version() != version {
                    return;
                }
                match result.map(|v| parse_prepare_rename(&v)) {
                    Ok(PrepareRename::Range { range, placeholder }) => {
                        let range = range_to_offsets(this.state.buffer(), range, encoding);
                        this.open_rename_input(range, placeholder, window, cx);
                    }
                    Ok(PrepareRename::DefaultBehavior) if !word.is_empty() => {
                        this.open_rename_input(word, None, window, cx);
                    }
                    Ok(_) => this.notice("Nothing to rename here", cx),
                    Err(error) => this.notice(format!("Can't rename here: {error}"), cx),
                }
            })
            .ok();
        }));
    }

    fn open_rename_input(
        &mut self,
        range: Range<usize>,
        placeholder: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let original = self.state.buffer().slice(range.clone());
        let text = placeholder.unwrap_or_else(|| original.clone());
        let len = text.len();
        let input = cx.new(|cx| TextInput::new(window, cx).with_text(text).monospace());
        input.update(cx, |input, cx| {
            input.select_range(0..len, cx);
            input.focus(window, cx);
        });
        let subscription =
            cx.subscribe_in(&input, window, |this, _, event, window, cx| match event {
                TextInputEvent::Submit => this.submit_rename(window, cx),
                // Clicking elsewhere cancels.
                TextInputEvent::Blurred => {
                    if this.actions.rename.take().is_some() {
                        cx.notify();
                    }
                }
                TextInputEvent::Changed => {}
            });
        self.hide_hover(cx);
        self.completion = None;
        self.actions.rename = Some(RenameInput {
            range,
            input,
            original,
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Escape in the rename input.
    pub(super) fn cancel_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.actions.rename.take().is_some() {
            self.focus(window, cx);
            cx.notify();
        }
    }

    /// Whether the rename input has focus (editor key bindings then leave
    /// the buffer alone).
    pub(super) fn rename_focused(&self, window: &Window, cx: &Context<Self>) -> bool {
        self.actions
            .rename
            .as_ref()
            .is_some_and(|r| r.input.focus_handle(cx).is_focused(window))
    }

    /// Enter in the rename input: asks the server for the edit.
    fn submit_rename(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rename) = self.actions.rename.take() else {
            return;
        };
        let new_name = rename.input.read(cx).text().trim().to_string();
        self.focus(window, cx);
        cx.notify();
        if new_name.is_empty() || new_name == rename.original {
            return;
        }
        self.flush_lsp_changes();
        let Some(doc) = self.lsp.doc.as_ref() else {
            return;
        };
        let encoding = doc.client.position_encoding();
        let position = offset_to_position(self.state.buffer(), rename.range.start, encoding);
        let request = doc.client.request(
            "textDocument/rename",
            json!({
                "textDocument": { "uri": doc.uri },
                "position": position.to_json(),
                "newName": new_name,
            }),
        );
        self.actions.rename_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| match result {
                Ok(value) => {
                    let edit = parse_workspace_edit(&value);
                    if edit.is_empty() {
                        this.notice("Nothing to rename", cx);
                    } else {
                        cx.emit(EditorEvent::PreviewEdit {
                            edit,
                            encoding,
                            label: format!("Renamed to `{new_name}`"),
                        });
                    }
                }
                Err(error) => this.notice(format!("Rename failed: {error}"), cx),
            })
            .ok();
        }));
    }

    // ----- References -----------------------------------------------------------

    /// Shift+F12: every reference to the symbol, listed by the workspace.
    pub(super) fn find_references(&mut self, cx: &mut Context<Self>) {
        if !self.ready_for("referencesProvider", "find references", cx) {
            return;
        }
        let head = self.state.primary().head;
        let symbol = self
            .state
            .buffer()
            .slice(identifier_at(self.state.buffer(), head));
        let (Some(mut params), Some(doc)) = (self.position_params(head), self.lsp.doc.as_ref())
        else {
            return;
        };
        params["context"] = json!({ "includeDeclaration": true });
        let encoding = doc.client.position_encoding();
        let request = doc.client.request("textDocument/references", params);
        self.actions.references_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| match result {
                Ok(value) => {
                    let locations = parse_locations(&value);
                    if locations.is_empty() {
                        this.notice("No references found", cx);
                    } else {
                        cx.emit(EditorEvent::References {
                            locations,
                            encoding,
                            symbol,
                        });
                    }
                }
                Err(error) => this.notice(format!("Find references failed: {error}"), cx),
            })
            .ok();
        }));
    }

    // ----- Code actions -----------------------------------------------------------

    /// Cmd+.: quick fixes and refactorings for the selection (with the
    /// problems under it).
    pub(super) fn show_code_actions(&mut self, cx: &mut Context<Self>) {
        if !self.ready_for("codeActionProvider", "offer code actions", cx) {
            return;
        }
        let Some(doc) = self.lsp.doc.as_ref() else {
            return;
        };
        let encoding = doc.client.position_encoding();
        let selection = self.state.primary();
        let (start, end) = (selection.start(), selection.end());
        let diagnostics: Vec<Value> = self
            .lsp
            .diagnostics
            .iter()
            .filter(|d| d.range.start <= end && start <= d.range.end && !d.raw.is_null())
            .map(|d| d.raw.clone())
            .collect();
        let buffer = self.state.buffer();
        let request = doc.client.request(
            "textDocument/codeAction",
            json!({
                "textDocument": { "uri": doc.uri },
                "range": {
                    "start": offset_to_position(buffer, start, encoding).to_json(),
                    "end": offset_to_position(buffer, end, encoding).to_json(),
                },
                "context": { "diagnostics": diagnostics, "triggerKind": 1 },
            }),
        );
        self.actions.action_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| match result {
                Ok(value) => {
                    let actions = parse_code_actions(&value);
                    if actions.is_empty() {
                        this.notice("No code actions here", cx);
                    } else {
                        cx.emit(EditorEvent::CodeActions(actions));
                    }
                }
                Err(error) => this.notice(format!("Code actions failed: {error}"), cx),
            })
            .ok();
        }));
    }

    /// Runs a chosen action: resolves it when the server sent only its
    /// title, then hands its edit and command to the workspace.
    pub fn run_code_action(&mut self, action: CodeAction, cx: &mut Context<Self>) {
        let Some(doc) = self.lsp.doc.as_ref() else {
            return;
        };
        let encoding = doc.client.position_encoding();
        let resolvable = self.capabilities()["codeActionProvider"]["resolveProvider"] == true;
        if !(action.needs_resolve() && resolvable) {
            emit_action(action, encoding, cx);
            return;
        }
        let request = doc.client.request("codeAction/resolve", action.raw.clone());
        self.actions.action_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| match result {
                Ok(value) => {
                    let mut action = action;
                    action.apply_resolved(&value);
                    emit_action(action, encoding, cx);
                }
                Err(error) => this.notice(format!("Code action failed: {error}"), cx),
            })
            .ok();
        }));
    }

    /// Runs a server command (a code action's, after its edit).
    pub fn execute_command(&mut self, command: LspCommand, cx: &mut Context<Self>) {
        let Some(doc) = self.lsp.doc.as_ref() else {
            return;
        };
        let request = doc
            .client
            .request("workspace/executeCommand", command.params());
        self.actions.action_task = Some(cx.spawn(async move |this, cx| {
            if let Err(error) = request.await {
                this.update(cx, |this, cx| {
                    this.notice(format!("“{}” failed: {error}", command.title), cx);
                })
                .ok();
            }
        }));
    }

    // ----- Formatting -------------------------------------------------------------

    /// Formats the document with the server. `explicit` (the command)
    /// explains why it can't; format on save stays quiet. The task ends
    /// once the edits are applied, or after a timeout; `None` means there
    /// is nothing to wait for.
    pub fn format_document(&mut self, explicit: bool, cx: &mut Context<Self>) -> Option<Task<()>> {
        if explicit {
            if !self.ready_for("documentFormattingProvider", "format this file", cx) {
                return None;
            }
        } else if !self.has_language_server()
            || !has_capability(&self.capabilities(), "documentFormattingProvider")
        {
            return None;
        } else {
            self.flush_lsp_changes();
        }
        let doc = self.lsp.doc.as_ref()?;
        let encoding = doc.client.position_encoding();
        let settings = &self.state.settings;
        let request = doc.client.request(
            "textDocument/formatting",
            json!({
                "textDocument": { "uri": doc.uri },
                "options": {
                    "tabSize": settings.tab_size,
                    "insertSpaces": settings.soft_tabs,
                },
            }),
        );
        let version = self.state.buffer().version();
        let task = cx.spawn(async move |this, cx| {
            let timer = cx.background_executor().timer(FORMAT_TIMEOUT);
            let result = match futures::future::select(Box::pin(request), Box::pin(timer)).await {
                futures::future::Either::Left((result, _)) => Some(result),
                futures::future::Either::Right(_) => None,
            };
            this.update(cx, |this, cx| {
                // Typing meanwhile wins over a stale format.
                if this.state.buffer().version() != version {
                    return;
                }
                let applied = match result {
                    Some(Ok(value)) => {
                        this.apply_lsp_edits(&parse_text_edits(&value), encoding, cx)
                    }
                    Some(Err(error)) => Err(error),
                    None => Err("the language server took too long".to_string()),
                };
                match applied {
                    Ok(0) if explicit => this.notice("Already formatted", cx),
                    Ok(_) => {}
                    Err(error) => this.notice(format!("Formatting skipped: {error}"), cx),
                }
            })
            .ok();
        });
        if explicit {
            self.actions.format_task = Some(task);
            None
        } else {
            Some(task)
        }
    }

    // ----- Signature help -----------------------------------------------------------

    /// Characters that open (or update) signature help.
    fn signature_triggers(&self) -> Option<Vec<String>> {
        let caps = self.capabilities();
        let provider = &caps["signatureHelpProvider"];
        if !has_capability(&caps, "signatureHelpProvider") {
            return None;
        }
        let mut chars: Vec<String> = ["triggerCharacters", "retriggerCharacters"]
            .iter()
            .flat_map(|key| provider[*key].as_array().cloned().unwrap_or_default())
            .filter_map(|c| c.as_str().map(str::to_string))
            .collect();
        if chars.is_empty() {
            chars = vec!["(".into(), ",".into()];
        }
        Some(chars)
    }

    /// After typing: `(` and `,` ask for signature help, `)` closes it,
    /// anything else updates an open popup.
    pub(super) fn signature_after_typing(&mut self, typed: &str, cx: &mut Context<Self>) {
        let Some(last) = typed.chars().next_back() else {
            return;
        };
        let Some(triggers) = self.signature_triggers() else {
            return;
        };
        if last == ')' && self.actions.signature.is_some() {
            self.hide_signature_help(cx);
            return;
        }
        let trigger = triggers.iter().any(|t| t.ends_with(last));
        if trigger || self.actions.signature.is_some() {
            self.request_signature_help(trigger.then_some(last), cx);
        }
    }

    /// Asks for signature help at the caret (`trigger` is the typed
    /// character; `None` from the command or while the popup is open).
    pub(super) fn request_signature_help(&mut self, trigger: Option<char>, cx: &mut Context<Self>) {
        if self.signature_triggers().is_none() {
            return;
        }
        self.flush_lsp_changes();
        let head = self.state.primary().head;
        let (Some(mut params), Some(doc)) = (self.position_params(head), self.lsp.doc.as_ref())
        else {
            return;
        };
        params["context"] = match trigger {
            Some(c) => json!({ "triggerKind": 2, "triggerCharacter": c.to_string(),
                               "isRetrigger": self.actions.signature.is_some() }),
            None => json!({ "triggerKind": 1, "isRetrigger": self.actions.signature.is_some() }),
        };
        let version = self.state.buffer().version();
        let request = doc.client.request("textDocument/signatureHelp", params);
        self.actions.signature_task = Some(cx.spawn(async move |this, cx| {
            let result = request.await;
            this.update(cx, |this, cx| {
                if this.state.buffer().version() != version {
                    return;
                }
                this.actions.signature = result
                    .ok()
                    .as_ref()
                    .and_then(parse_signature_help)
                    .map(|help| SignaturePopup { help, anchor: head });
                cx.notify();
            })
            .ok();
        }));
    }

    pub(super) fn hide_signature_help(&mut self, cx: &mut Context<Self>) {
        self.actions.signature_task = None;
        if self.actions.signature.take().is_some() {
            cx.notify();
        }
    }
}

fn emit_action(action: CodeAction, encoding: PositionEncoding, cx: &mut Context<EditorView>) {
    cx.emit(EditorEvent::ApplyEdit {
        edit: action.edit.unwrap_or_default(),
        encoding,
        label: action.title,
        command: action.command,
    });
}

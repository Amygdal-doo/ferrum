//! Inline agent edits: ⌘K (or Code › Edit with Agent…) on a selection (or
//! the caret's line) opens a prompt over it; the model of the Agent pane rewrites those lines
//! (`ferrumeditor_core::inline_edit`). The answer goes into the buffer as
//! one undo step and stays under review: its lines are tinted, the lines
//! it replaced are listed below it, and Accept (⌘Enter) keeps it while
//! Reject (Escape) puts the old text back. Typing elsewhere accepts.

use std::ops::Range;
use std::sync::{Arc, Mutex};

use ferrum_ui::{TextInput, TextInputEvent};
use ferrumagent_ui::core::models::ModelStore;
use ferrumagent_ui::core::settings::AgentSettings;
use ferrumeditor_core::inline_edit::{self, InlineEditRequest};
use gpui::{
    App, AppContext as _, Context, Entity, Focusable as _, Global, KeyDownEvent, Modifiers,
    Subscription, Task, Window,
};

use super::{EditorEvent, EditorView};

/// The model choice of the Agent pane's current thread (`provider/model`
/// or `acp:<id>`), kept current by the workspace.
#[derive(Default)]
pub struct PaneAgent(pub Option<String>);

impl Global for PaneAgent {}

/// Providers and keys of "Models and API keys" (`~/.ferrum/agent`); tests
/// install one with a scripted provider.
pub struct InlineModels(pub ModelStore);

impl Global for InlineModels {}

fn model_store(cx: &mut App) -> ModelStore {
    if let Some(models) = cx.try_global::<InlineModels>() {
        return models.0.clone();
    }
    let store = ModelStore::new(
        ferrumagent_ui::core::paths::agent_home(),
        ferrumagent_ui::core::secrets::default_store(),
        Arc::new(Mutex::new(AgentSettings::default())),
        None,
        Vec::new(),
    );
    cx.set_global(InlineModels(store.clone()));
    store
}

pub(super) enum Phase {
    /// The prompt is open.
    Prompt,
    /// Waiting for the model (the prompt stays, read-only).
    Running { _task: Task<()> },
    /// The answer is in the buffer at `range`; `old` is what it replaced.
    Review { old: String },
}

pub(super) struct InlineEdit {
    /// The lines being rewritten (after the answer: the new text).
    pub(super) range: Range<usize>,
    pub(super) input: Entity<TextInput>,
    pub(super) phase: Phase,
    /// The model's name, for the prompt's footer.
    pub(super) model: String,
    pub(super) error: Option<String>,
    /// Buffer version the range belongs to.
    version: u64,
    _subscription: Subscription,
}

impl InlineEdit {
    pub(super) fn reviewing(&self) -> Option<&str> {
        match &self.phase {
            Phase::Review { old } => Some(old),
            _ => None,
        }
    }

    pub(super) fn running(&self) -> bool {
        matches!(self.phase, Phase::Running { .. })
    }
}

impl EditorView {
    /// ⌘K on its own. It also starts chords (⌘K ⌘0…), so it is not a key
    /// binding: GPUI waits for a second key, and when none extends a chord
    /// it hands ⌘K to this key handler.
    pub(super) fn on_inline_edit_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let keystroke = &event.keystroke;
        if keystroke.key == "k" && keystroke.modifiers == Modifiers::secondary_key() {
            self.start_inline_edit(window, cx);
            cx.stop_propagation();
        }
    }

    /// ⌘K: the prompt over the selected lines (or the caret's line).
    pub(super) fn start_inline_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inline.is_some() {
            self.cancel_inline_edit(window, cx);
        }
        let buffer = self.state.buffer();
        let selection = self.state.primary().range();
        let start = buffer.line_start(buffer.row_of(selection.start));
        let last_row =
            if selection.end > selection.start && buffer.offset_to_point(selection.end).col == 0 {
                buffer.row_of(selection.end) - 1
            } else {
                buffer.row_of(selection.end)
            };
        // Whole lines, with the last one's newline.
        let end = (buffer.line_end(last_row) + 1).min(buffer.len());
        let store = model_store(cx);
        let (provider, model) = inline_edit::pick_model(
            cx.try_global::<PaneAgent>().and_then(|a| a.0.as_deref()),
            &store,
        );
        let input = cx.new(|cx| {
            TextInput::new(window, cx).placeholder("Edit these lines: describe the change…")
        });
        input.update(cx, |input, cx| input.focus(window, cx));
        let subscription = cx.subscribe_in(&input, window, |this, _, event, window, cx| {
            if *event == TextInputEvent::Submit {
                this.submit_inline_edit(window, cx);
            }
        });
        self.hide_hover(cx);
        self.completion = None;
        self.inline = Some(InlineEdit {
            range: start..end,
            input,
            phase: Phase::Prompt,
            model: format!("{provider}/{model}"),
            error: None,
            version: self.state.buffer().version(),
            _subscription: subscription,
        });
        cx.notify();
    }

    /// Text that reached the editor while the prompt is open (keys typed
    /// right after ⌘K are replayed to the editor): it goes to the prompt.
    /// True when taken.
    pub(super) fn type_into_inline_prompt(
        &mut self,
        text: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(inline) = self
            .inline
            .as_ref()
            .filter(|i| matches!(i.phase, Phase::Prompt))
        else {
            return false;
        };
        inline
            .input
            .update(cx, |input, cx| input.insert(text, window, cx));
        true
    }

    /// Whether the inline prompt has focus (editor keys then leave the
    /// buffer alone).
    pub(super) fn inline_prompt_focused(&self, window: &Window, cx: &App) -> bool {
        self.inline
            .as_ref()
            .is_some_and(|i| i.input.focus_handle(cx).is_focused(window))
    }

    /// Enter in the prompt: asks the model.
    fn submit_inline_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inline) = self.inline.as_mut() else {
            return;
        };
        let instruction = inline.input.read(cx).text().trim().to_string();
        if instruction.is_empty() || !matches!(inline.phase, Phase::Prompt) {
            return;
        }
        let request = InlineEditRequest::new(
            &self.state.text(),
            inline.range.clone(),
            &instruction,
            self.path.clone(),
            self.state.language,
        );
        let (provider, model) = inline
            .model
            .split_once('/')
            .map(|(p, m)| (p.to_string(), m.to_string()))
            .unwrap_or_default();
        let answer = inline_edit::run(model_store(cx), provider, model, request);
        inline.error = None;
        inline.phase = Phase::Running {
            _task: cx.spawn_in(window, async move |this, cx| {
                let answer = answer.await;
                this.update_in(cx, |this, window, cx| {
                    this.inline_answer(answer, window, cx)
                })
                .ok();
            }),
        };
        cx.notify();
    }

    fn inline_answer(
        &mut self,
        answer: Result<String, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(inline) = self.inline.as_mut() else {
            return;
        };
        let text = match answer {
            Ok(text) if inline.version == self.state.buffer().version() => text,
            Ok(_) => {
                inline.error = Some("The file changed while the model was working.".into());
                inline.phase = Phase::Prompt;
                cx.notify();
                return;
            }
            Err(error) => {
                inline.error = Some(error);
                inline.phase = Phase::Prompt;
                cx.notify();
                return;
            }
        };
        let range = inline.range.clone();
        let old = self.state.buffer().slice(range.clone());
        self.state
            .replace_ranges(vec![(range.clone(), text.clone())]);
        let new_range = range.start..range.start + text.len();
        self.state.set_cursor(new_range.start);
        if let Some(inline) = self.inline.as_mut() {
            inline.range = new_range;
            inline.phase = Phase::Review { old };
            inline.version = self.state.buffer().version();
        }
        self.after_edit(cx);
        self.request_autoscroll();
        // Review keys (⌘Enter, Escape) go to the editor.
        self.focus(window, cx);
    }

    /// ⌘Enter while reviewing: keep the new text.
    pub(super) fn accept_inline_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.inline.take().is_some() {
            self.focus(window, cx);
            cx.notify();
        }
    }

    /// Escape: closes the prompt, or puts the old text back when reviewing.
    pub(super) fn cancel_inline_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(inline) = self.inline.take() else {
            return;
        };
        if let Phase::Review { old } = inline.phase
            && inline.version == self.state.buffer().version()
        {
            self.state.replace_ranges(vec![(inline.range.clone(), old)]);
            self.state.set_cursor(inline.range.start);
            self.after_edit(cx);
        }
        self.focus(window, cx);
        cx.notify();
    }

    /// Editor commands while an inline edit is open. True when handled.
    pub(super) fn inline_edit_command(
        &mut self,
        name: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if name == "editor:inline-edit" {
            self.start_inline_edit(window, cx);
            return true;
        }
        let Some(inline) = &self.inline else {
            return false;
        };
        let reviewing = inline.reviewing().is_some();
        match name {
            "editor:cancel" | "editor:reject-inline-edit" => {
                self.cancel_inline_edit(window, cx);
                true
            }
            "editor:accept-inline-edit" => {
                self.accept_inline_edit(window, cx);
                true
            }
            "editor:newline-below" if reviewing => {
                self.accept_inline_edit(window, cx);
                true
            }
            // Keys typed into the prompt must not edit the buffer.
            _ if self.inline_prompt_focused(window, cx) => true,
            _ => {
                // Editing on: the reviewed text is kept.
                if reviewing {
                    self.inline = None;
                    cx.emit(EditorEvent::SelectionsChanged);
                }
                false
            }
        }
    }
}

#[cfg(test)]
impl EditorView {
    /// (reviewing, running, error) of the open inline edit.
    pub fn inline_state(&self) -> Option<(bool, bool, Option<String>)> {
        self.inline
            .as_ref()
            .map(|i| (i.reviewing().is_some(), i.running(), i.error.clone()))
    }
}

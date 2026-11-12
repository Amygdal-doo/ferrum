//! Keyed cache of `TextInput` entities. The root view renders everything
//! from `AppState`; text fields are the only stateful widgets, so they are
//! created on first use, kept while rendered, and dropped once a render no
//! longer asks for them. Controlled fields are re-synced from state on each
//! render, which replaces the Dioxus "remount on rev bump" trick.

use crate::app::FerrumApi;
use ferrum_ui::{TextInput, TextInputEvent};
use gpui::{AppContext as _, Context, Entity, Focusable as _, SharedString, Subscription, Window};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FieldEvent {
    Changed(String),
    /// Enter (single-line) or cmd/ctrl-Enter.
    Submit,
    Blurred,
}

pub type FieldHandler =
    Rc<dyn Fn(&mut FerrumApi, FieldEvent, &mut Window, &mut Context<FerrumApi>)>;

pub struct FieldSpec {
    key: SharedString,
    value: Option<String>,
    initial: String,
    placeholder: SharedString,
    masked: bool,
    multiline: Option<usize>,
    monospace: bool,
    borderless: bool,
    read_only: bool,
    autofocus: bool,
}

impl FieldSpec {
    pub fn new(key: impl Into<SharedString>) -> Self {
        Self {
            key: key.into(),
            value: None,
            initial: String::new(),
            placeholder: SharedString::default(),
            masked: false,
            multiline: None,
            monospace: false,
            borderless: false,
            read_only: false,
            autofocus: false,
        }
    }

    /// Controlled: the field always shows `value` (state is the source of truth).
    pub fn value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Uncontrolled: `initial` is used only when the field is created.
    pub fn initial(mut self, initial: impl Into<String>) -> Self {
        self.initial = initial.into();
        self
    }

    pub fn placeholder(mut self, placeholder: impl Into<SharedString>) -> Self {
        self.placeholder = placeholder.into();
        self
    }

    pub fn masked(mut self, masked: bool) -> Self {
        self.masked = masked;
        self
    }

    pub fn multiline(mut self, min_lines: usize) -> Self {
        self.multiline = Some(min_lines);
        self
    }

    pub fn mono(mut self) -> Self {
        self.monospace = true;
        self
    }

    pub fn borderless(mut self) -> Self {
        self.borderless = true;
        self
    }

    /// Selectable and copyable, but not editable.
    pub fn read_only(mut self) -> Self {
        self.read_only = true;
        self
    }

    pub fn autofocus(mut self) -> Self {
        self.autofocus = true;
        self
    }
}

struct Entry {
    input: Entity<TextInput>,
    handler: Rc<RefCell<FieldHandler>>,
    generation: u64,
    masked: bool,
    placeholder: SharedString,
    _subscription: Subscription,
}

#[derive(Default)]
pub struct FieldCache {
    entries: HashMap<SharedString, Entry>,
    generation: u64,
}

impl FieldCache {
    /// Starts a render pass.
    pub fn begin(&mut self) {
        self.generation += 1;
    }

    /// Drops fields the finished render pass did not use.
    pub fn sweep(&mut self) {
        let generation = self.generation;
        self.entries
            .retain(|_, entry| entry.generation == generation);
    }

    /// The live field for `key`, if rendered this pass or the last.
    pub fn entity(&self, key: &str) -> Option<Entity<TextInput>> {
        self.entries.get(key).map(|entry| entry.input.clone())
    }

    pub fn text(&self, key: &str, cx: &gpui::App) -> Option<String> {
        self.entries
            .get(key)
            .map(|entry| entry.input.read(cx).text().to_owned())
    }
}

impl FerrumApi {
    /// Returns the text field for `spec.key`, creating it on first use.
    /// `handler` replaces the previous one each render, so it may capture
    /// current values.
    pub fn field(
        &mut self,
        spec: FieldSpec,
        handler: impl Fn(&mut FerrumApi, FieldEvent, &mut Window, &mut Context<FerrumApi>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let handler: FieldHandler = Rc::new(handler);
        let generation = self.fields.generation;

        if let Some(entry) = self.fields.entries.get_mut(&spec.key) {
            entry.generation = generation;
            *entry.handler.borrow_mut() = handler;
            let input = entry.input.clone();
            if entry.masked != spec.masked {
                entry.masked = spec.masked;
                input.update(cx, |input, cx| input.set_masked(spec.masked, cx));
            }
            if entry.placeholder != spec.placeholder {
                entry.placeholder = spec.placeholder.clone();
                let placeholder = spec.placeholder;
                input.update(cx, |input, cx| input.set_placeholder(placeholder, cx));
            }
            if let Some(value) = spec.value
                && input.read(cx).text() != value
            {
                input.update(cx, |input, cx| input.set_text(value, cx));
            }
            return input;
        }

        let text = spec.value.clone().unwrap_or(spec.initial);
        let input = cx.new(|cx| {
            let mut input = TextInput::new(window, cx)
                .placeholder(spec.placeholder.clone())
                .masked(spec.masked);
            if let Some(lines) = spec.multiline {
                input = input.multiline(lines);
            }
            if spec.monospace {
                input = input.monospace();
            }
            if spec.borderless {
                input = input.borderless();
            }
            input.read_only(spec.read_only).with_text(text)
        });
        let handler = Rc::new(RefCell::new(handler));
        let subscription = cx.subscribe_in(&input, window, {
            let handler = handler.clone();
            move |this, input, event, window, cx| {
                let event = match event {
                    TextInputEvent::Changed => {
                        FieldEvent::Changed(input.read(cx).text().to_owned())
                    }
                    TextInputEvent::Submit => FieldEvent::Submit,
                    TextInputEvent::Blurred => FieldEvent::Blurred,
                };
                let handler = handler.borrow().clone();
                handler(this, event, window, cx);
                cx.notify();
            }
        });
        if spec.autofocus {
            let focus = input.focus_handle(cx);
            window.focus(&focus, cx);
        }
        self.fields.entries.insert(
            spec.key,
            Entry {
                input: input.clone(),
                handler,
                generation,
                masked: spec.masked,
                placeholder: spec.placeholder,
                _subscription: subscription,
            },
        );
        input
    }
}

/// Handler that applies only `Changed` text.
pub fn on_change(
    apply: impl Fn(&mut FerrumApi, String, &mut Context<FerrumApi>) + 'static,
) -> impl Fn(&mut FerrumApi, FieldEvent, &mut Window, &mut Context<FerrumApi>) + 'static {
    move |this, event, _, cx| {
        if let FieldEvent::Changed(text) = event {
            apply(this, text, cx);
        }
    }
}

/// Handler for request-editing fields: text edits plus Enter / cmd-Enter sends.
pub fn on_request_edit(
    apply: impl Fn(&mut FerrumApi, String, &mut Context<FerrumApi>) + 'static,
) -> impl Fn(&mut FerrumApi, FieldEvent, &mut Window, &mut Context<FerrumApi>) + 'static {
    move |this, event, _, cx| match event {
        FieldEvent::Changed(text) => apply(this, text, cx),
        FieldEvent::Submit => {
            if this.state.dialog.is_none() {
                this.send_or_cancel_active(cx);
            }
        }
        FieldEvent::Blurred => {}
    }
}

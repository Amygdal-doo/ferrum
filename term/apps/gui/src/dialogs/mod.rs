//! Modal dialogs over the terminal (`ferrum-ui` `Modal` + `TextInput`): the
//! SSH host form (with its port forwards) and "What's using a port?".

pub mod host_form;
pub mod ports;

use crate::view::TermView;
use ferrum_ui::components::{Color, Label, LabelCommon, LabelSize, v_flex};
use ferrum_ui::gpui::{AnyElement, Context, Div, Entity, SharedString, Window, prelude::*};
use ferrum_ui::{Modal, TextInput, TextInputEvent};

pub enum Dialog {
    Host(host_form::HostForm),
    Ports(ports::PortsDialog),
}

/// A labelled form row: small muted caption above `child`.
pub(crate) fn field(label: impl Into<SharedString>, child: impl IntoElement) -> Div {
    v_flex()
        .gap_1()
        .child(
            Label::new(label.into())
                .size(LabelSize::Small)
                .color(Color::Muted),
        )
        .child(child)
}

pub(crate) fn hint(text: impl Into<SharedString>) -> Label {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(Color::Muted)
}

pub(crate) fn error_text(text: impl Into<SharedString>) -> Label {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(Color::Error)
}

impl TermView {
    /// A text field that re-renders on edits and submits the dialog on Enter.
    pub(crate) fn dialog_input(
        &mut self,
        placeholder: &str,
        text: &str,
        read_only: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let (placeholder, text) = (placeholder.to_owned(), text.to_owned());
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder(placeholder)
                .with_text(text)
                .read_only(read_only)
        });
        let subscription =
            cx.subscribe_in(&input, window, |view, _, event, window, cx| match event {
                TextInputEvent::Changed => cx.notify(),
                TextInputEvent::Submit => view.submit_dialog(window, cx),
                TextInputEvent::Blurred => {}
            });
        self._dialog_events.push(subscription);
        input
    }

    pub(crate) fn open_dialog(
        &mut self,
        dialog: Dialog,
        focus: Option<Entity<TextInput>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.end_custom_agent(window, cx);
        self.dialog = Some(dialog);
        if let Some(input) = focus {
            input.update(cx, |input, cx| input.focus(window, cx));
        }
        cx.notify();
    }

    pub(crate) fn close_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.dialog.take().is_some() {
            self._dialog_events.clear();
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    /// Enter in a field: the dialog's primary action.
    pub(crate) fn submit_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match &self.dialog {
            Some(Dialog::Host(_)) => self.save_host_form(window, cx),
            Some(Dialog::Ports(_)) => self.run_port_query(cx),
            None => {}
        }
    }

    pub(crate) fn render_dialog(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        Some(match self.dialog.as_ref()? {
            Dialog::Host(form) => self.render_host_form(form, cx),
            Dialog::Ports(dialog) => self.render_ports(dialog, cx),
        })
    }
}

pub(crate) fn modal(title: impl Into<SharedString>, cx: &mut Context<TermView>) -> Modal {
    Modal::new(title).on_dismiss(cx.listener(|view, _, window, cx| view.close_dialog(window, cx)))
}

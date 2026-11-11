//! Add / edit a variable: key, value (masked while it is a secret),
//! comment and the secret flag.

use ferrum_ui::prelude::*;
use ferrum_ui::ui::Checkbox;
use ferrumenv_core::Variable;
use zeroize::Zeroizing;

use super::{Dialog, input};
use crate::app::EnvApp;
use crate::widgets::{field, muted, primary_button, text_button};

pub(crate) struct VariableDialog {
    /// `None` adds; `Some(i)` edits variable `i` of the current environment.
    pub(crate) index: Option<usize>,
    pub(crate) key: Entity<TextInput>,
    pub(crate) value: Entity<TextInput>,
    pub(crate) comment: Entity<TextInput>,
    pub(crate) secret: bool,
    pub(crate) show_value: bool,
}

impl EnvApp {
    pub(crate) fn open_variable_dialog(
        &mut self,
        index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(env) = self.vault().and_then(|u| u.environment()) else {
            return;
        };
        let existing = index.and_then(|i| env.variables.get(i)).cloned();
        if index.is_some() && existing.is_none() {
            return;
        }
        let var = existing.unwrap_or_else(|| Variable::new("", ""));
        let mut subscriptions = Vec::new();
        let key = input(window, cx, &mut subscriptions, |t| {
            t.monospace()
                .placeholder("DATABASE_URL")
                .with_text(var.key.clone())
        });
        let value = input(window, cx, &mut subscriptions, |t| {
            t.monospace()
                .multiline(2)
                .masked(var.secret)
                .placeholder("value (⌘↵ saves)")
                .with_text(var.value.clone())
        });
        let comment = input(window, cx, &mut subscriptions, |t| {
            t.placeholder("optional").with_text(var.comment.clone())
        });
        let focus = if index.is_some() {
            value.clone()
        } else {
            key.clone()
        };
        let dialog = VariableDialog {
            index,
            key,
            value,
            comment,
            secret: var.secret,
            show_value: !var.secret,
        };
        self.open_dialog(
            Dialog::Variable(dialog),
            subscriptions,
            Some(focus),
            window,
            cx,
        );
    }

    pub(crate) fn submit_variable(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Variable(d)) = self.dialog_mut() else {
            return;
        };
        let mut var = Variable::new(
            d.key.read(cx).text().trim(),
            Zeroizing::new(d.value.read(cx).text().to_string()).as_str(),
        );
        var.comment = d.comment.read(cx).text().trim().to_string();
        var.secret = d.secret;
        let index = d.index;
        let saved = self.change(cx, |u| {
            let env = u.environment_mut().ok_or("No environment selected")?;
            env.set(index, var).map_err(|e| e.to_string())
        });
        if saved.is_some() {
            self.close_dialog(window, cx);
        }
    }

    fn toggle_value_visible(&mut self, cx: &mut Context<Self>) {
        if let Some(Dialog::Variable(d)) = self.dialog_mut() {
            d.show_value = !d.show_value;
            let masked = !d.show_value;
            d.value.update(cx, |input, cx| input.set_masked(masked, cx));
        }
        cx.notify();
    }

    pub(crate) fn render_variable(&self, d: &VariableDialog, cx: &mut Context<Self>) -> Modal {
        let env_name = self
            .vault()
            .and_then(|u| u.environment())
            .map(|e| e.name.clone())
            .unwrap_or_default();
        let title = match d.index {
            Some(_) => format!("Edit variable · {env_name}"),
            None => format!("Add variable · {env_name}"),
        };
        let secret = d.secret;
        self.modal(title, cx)
            .width(px(560.))
            .child(field("Name", d.key.clone()))
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        h_flex()
                            .justify_between()
                            .child(muted("Value"))
                            .child(text_button(
                                "toggle-value",
                                if d.show_value { "Hide" } else { "Show" },
                                cx,
                                |this, _, cx| this.toggle_value_visible(cx),
                            )),
                    )
                    .child(d.value.clone()),
            )
            .child(field("Comment", d.comment.clone()))
            .child(
                Checkbox::new("secret", ToggleState::from(secret))
                    .label("Secret: mask the value in the table")
                    .label_size(LabelSize::Small)
                    .on_click(cx.listener(|this, _: &ToggleState, _, cx| {
                        if let Some(Dialog::Variable(d)) = this.dialog_mut() {
                            d.secret = !d.secret;
                        }
                        cx.notify();
                    })),
            )
            .footer(primary_button(
                "save-variable",
                "Save",
                cx,
                |this, window, cx| {
                    this.submit_variable(window, cx);
                },
            ))
    }
}

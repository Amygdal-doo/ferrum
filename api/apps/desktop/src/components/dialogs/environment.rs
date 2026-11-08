//! Environment / globals variable editor.

use super::{actions, modal};
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::widgets::RowEditor;
use crate::components::{checkbox, hint};
use crate::state::Dialog;
use crate::state::kv;
use ferrumapi_domain::Environment;
use gpui::{AnyElement, Context, Window, div, prelude::*, px};
use std::rc::Rc;
use ui::{ActiveTheme as _, Label, LabelCommon as _, LabelSize, h_flex};

fn with_draft(this: &mut FerrumApi, f: impl FnOnce(&mut Environment)) {
    if let Some(Dialog::EditEnvironment(env)) = &mut this.state.dialog {
        f(env);
    }
}

impl FerrumApi {
    pub(super) fn render_environment_dialog(
        &mut self,
        env: Environment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let id = env.id;
        let globals = id.is_globals();
        let active = self.state.active_env == Some(id);
        let title = if globals { "Globals" } else { "Environment" };

        let header = if globals {
            hint("Globals are available to every request. Collection variables and the active environment override them.")
                .into_any_element()
        } else {
            let name = self.field(
                FieldSpec::new(format!("env-name:{id}")).value(env.name.clone()),
                on_change(|this, text, _| with_draft(this, |env| env.name = text)),
                window,
                cx,
            );
            h_flex()
                .gap_3()
                .child(
                    div()
                        .w_16()
                        .child(Label::new("Name").size(LabelSize::Small)),
                )
                .child(div().flex_1().child(name))
                .child(
                    h_flex()
                        .gap_1p5()
                        .child(checkbox(
                            "env-active",
                            active,
                            move |this, on, _| {
                                this.set_active_environment(on.then_some(id));
                            },
                            cx,
                        ))
                        .child(Label::new("Active").size(LabelSize::Small)),
                )
                .into_any_element()
        };
        let on_edit: RowEditor = Rc::new(|this, i, edit, _| {
            with_draft(this, |env| kv::apply(&mut env.variables, i, edit));
        });
        let table =
            self.render_vars_table(format!("env-vars:{id}"), env.variables, on_edit, window, cx);

        actions(
            modal(title, px(768.), cx)
                .child(header)
                .child(
                    div()
                        .min_h(px(192.))
                        .rounded_md()
                        .border_1()
                        .border_color(colors.border)
                        .child(table),
                )
                .child(hint(
                    "Use variables as {{name}} in the URL, params, headers, body and auth. Secret values are masked.",
                )),
            "Save",
            false,
            false,
            |this, cx| {
                if let Some(Dialog::EditEnvironment(env)) = this.state.dialog.take() {
                    this.save_environment(*env, cx);
                }
            },
            cx,
        )
        .into_any_element()
    }
}

//! Collection name, auth (inherited by its requests) and variables.

use super::{actions, modal};
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change};
use crate::components::widgets::{AuthTarget, AuthView, RowEditor};
use crate::components::{hint, tab_button};
use crate::state::kv;
use crate::state::{Dialog, SettingsPane};
use ferrum_ui::components::{ActiveTheme as _, Label, LabelCommon as _, LabelSize, h_flex, v_flex};
use ferrumapi_domain::{Auth, Collection};
use gpui::{AnyElement, Context, Window, div, prelude::*, px};
use std::rc::Rc;

fn with_draft(this: &mut FerrumApi, f: impl FnOnce(&mut Collection, &mut SettingsPane)) {
    if let Some(Dialog::CollectionSettings { draft, pane }) = &mut this.state.dialog {
        f(draft, pane);
    }
}

impl FerrumApi {
    pub(super) fn render_collection_settings(
        &mut self,
        c: Collection,
        pane: SettingsPane,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let id = c.id;
        // `Inherit` at the root means no auth; show it as such.
        let auth = if c.auth.is_inherit() {
            Auth::None
        } else {
            c.auth.clone()
        };
        let name = self.field(
            FieldSpec::new(format!("collection-name:{id}")).value(c.name.clone()),
            on_change(|this, text, _| with_draft(this, |c, _| c.name = text)),
            window,
            cx,
        );
        let pane_tab = |label: String, target: SettingsPane, cx: &mut Context<Self>| {
            tab_button(label.clone(), label, pane == target, cx).on_click(cx.listener(
                move |this, _, _, cx| {
                    with_draft(this, |_, pane| *pane = target);
                    cx.notify();
                },
            ))
        };
        let tabs = h_flex()
            .h_8()
            .gap_4()
            .border_b_1()
            .border_color(colors.border)
            .child(pane_tab("Authorization".to_owned(), SettingsPane::Auth, cx))
            .child(pane_tab(
                format!("Variables ({})", kv::active_count(&c.variables)),
                SettingsPane::Variables,
                cx,
            ));
        let body = match pane {
            SettingsPane::Auth => {
                let env: &[_] = self
                    .state
                    .active_environment()
                    .map_or(&[], |e| &e.variables);
                let variables = ferrumapi_domain::VariableSet::from_layers(&[
                    &self.state.globals.variables,
                    &c.variables,
                    env,
                ]);
                let target: AuthTarget = Rc::new(|this, f| {
                    with_draft(this, |c, _| {
                        if c.auth.is_inherit() {
                            c.auth = Auth::None;
                        }
                        f(&mut c.auth);
                    });
                });
                let editor = self.render_auth_editor(
                    AuthView {
                        auth,
                        scope: format!("collection-auth:{id}"),
                        allow_inherit: false,
                        inherited: None,
                        request_fields: false,
                        target,
                        variables,
                    },
                    window,
                    cx,
                );
                v_flex()
                    .gap_3()
                    .child(hint(
                        "Requests and folders set to “Inherit from parent” use this auth.",
                    ))
                    .child(editor)
                    .into_any_element()
            }
            SettingsPane::Variables => {
                let on_edit: RowEditor = Rc::new(|this, i, edit, _| {
                    with_draft(this, |c, _| kv::apply(&mut c.variables, i, edit));
                });
                let table = self.render_vars_table(
                    format!("collection-vars:{id}"),
                    c.variables.clone(),
                    on_edit,
                    window,
                    cx,
                );
                v_flex()
                    .gap_3()
                    .child(hint("Collection variables override globals; the active environment overrides both."))
                    .child(table)
                    .into_any_element()
            }
        };

        actions(
            modal("Collection settings", px(768.), cx)
                .child(
                    h_flex()
                        .gap_3()
                        .child(
                            div()
                                .w_24()
                                .child(Label::new("Name").size(LabelSize::Small)),
                        )
                        .child(div().flex_1().child(name)),
                )
                .child(tabs)
                .child(div().min_h(px(256.)).child(body)),
            "Save",
            false,
            false,
            |this, cx| {
                if let Some(Dialog::CollectionSettings { draft, .. }) = this.state.dialog.take() {
                    this.update_collection_meta(*draft, cx);
                }
            },
            cx,
        )
        .into_any_element()
    }
}

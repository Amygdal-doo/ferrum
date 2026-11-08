//! Environments list: globals, environments, active marker, row actions.

use super::menu::{MenuAction, more_button, with_context_menu};
use super::{icon_action, section_header};
use crate::app::FerrumApi;
use crate::components::{MenuHandler, menu_item};
use crate::state::Dialog;
use ferrumapi_domain::Environment;
use gpui::{AnyElement, Context, SharedString, Window, div, prelude::*};
use ui::{
    ActiveTheme as _, Color, Divider, IconName, Label, LabelCommon as _, LabelSize, Tooltip,
    VisibleOnHover as _, h_flex, v_flex,
};

impl FerrumApi {
    pub(super) fn render_environments_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let envs = self.state.environments.clone();
        let globals = self.state.globals.clone();
        let header = section_header(
            "Environments",
            vec![
                icon_action(
                    "import-environment",
                    IconName::Download,
                    "Import Postman environment",
                    cx,
                    |this, cx| this.import_environment(cx),
                ),
                icon_action(
                    "new-environment",
                    IconName::Plus,
                    "New environment",
                    cx,
                    |this, cx| this.create_environment(cx),
                ),
            ],
        );
        let globals_row = self.render_env_row(&globals, window, cx);
        let rows: Vec<_> = envs
            .iter()
            .map(|env| self.render_env_row(env, window, cx))
            .collect();

        v_flex()
            .size_full()
            .child(header)
            .child(
                v_flex()
                    .id("environments-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .child(globals_row)
                    .child(div().my_2().child(Divider::horizontal()))
                    .when(envs.is_empty(), |this| {
                        this.child(
                            div().px_2().py_4().child(
                                Label::new("No environments yet. Create one to switch between servers and credentials.")
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                        )
                    })
                    .children(rows),
            )
            .into_any_element()
    }

    fn render_env_row(
        &mut self,
        env: &Environment,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let id = env.id;
        let globals = id.is_globals();
        let active = self.state.active_env == Some(id);
        let renaming = self.state.renaming == Some(id.0);
        let count = env
            .variables
            .iter()
            .filter(|v| !v.key.trim().is_empty())
            .count();
        let actions = if globals {
            vec![MenuAction::Open]
        } else {
            vec![
                MenuAction::Open,
                MenuAction::Rename,
                MenuAction::Duplicate,
                MenuAction::Delete,
            ]
        };
        let name = env.name.clone();
        let entries: Vec<(SharedString, MenuHandler)> = actions
            .into_iter()
            .map(|action| {
                let name = name.clone();
                menu_item(
                    action.label(),
                    move |this: &mut FerrumApi, _, cx| match action {
                        MenuAction::Rename => this.state.renaming = Some(id.0),
                        MenuAction::Duplicate => this.duplicate_environment(id, cx),
                        MenuAction::Delete => {
                            this.state.dialog = Some(Dialog::DeleteEnvironment(id, name.clone()));
                        }
                        _ => this.edit_environment(id),
                    },
                )
            })
            .collect();

        let marker = if globals {
            div()
                .w_3()
                .text_xs()
                .text_color(colors.text_muted)
                .child("◎")
                .into_any_element()
        } else {
            div()
                .id(SharedString::from(format!("env-active-{id}")))
                .w_3()
                .text_xs()
                .cursor_pointer()
                .text_color(colors.text_muted)
                .hover(|s| s.text_color(colors.text))
                .tooltip(Tooltip::text(if active {
                    "Active environment (click to deactivate)"
                } else {
                    "Set as active environment"
                }))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.set_active_environment(if active { None } else { Some(id) });
                    cx.notify();
                }))
                .child(if active { "●" } else { "○" })
                .into_any_element()
        };
        let group: SharedString = format!("env-row-{id}").into();
        let body = if renaming {
            let input = self.rename_field(
                id.0,
                &name,
                move |this, new_name, cx| this.rename_environment(id, new_name, cx),
                window,
                cx,
            );
            div().flex_1().min_w_0().child(input).into_any_element()
        } else {
            h_flex()
                .flex_1()
                .min_w_0()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_sm()
                        .child(name.clone()),
                )
                .child(
                    Label::new(count.to_string())
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                )
                .child(div().visible_on_hover(group.clone()).child(more_button(
                    format!("env-more-{id}"),
                    entries.clone(),
                    cx,
                )))
                .into_any_element()
        };
        let row = h_flex()
            .id(SharedString::from(format!("env-row-{id}")))
            .group(group)
            .h_8()
            .gap_2()
            .px_2()
            .rounded_md()
            .map(|this| {
                if active {
                    this.bg(colors.element_selected)
                } else {
                    this.hover(|s| s.bg(colors.element_hover))
                }
            })
            .on_click(cx.listener(move |this, _, _, cx| {
                if this.state.renaming == Some(id.0) {
                    return;
                }
                this.edit_environment(id);
                cx.notify();
            }))
            .child(marker)
            .child(body)
            .into_any_element();
        with_context_menu(format!("env-menu-{id}"), row, entries, cx)
    }
}

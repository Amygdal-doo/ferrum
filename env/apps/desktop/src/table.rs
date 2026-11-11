//! The selected project: header, environment tabs, environment actions
//! and the variable table (values masked until revealed).

use ferrum_ui::prelude::*;
use ferrumenv_core::dotenv;

use crate::app::{EnvApp, NewVariable};
use crate::dialogs::ConfirmAction;
use crate::secrets::{MASK, preview};
use crate::widgets::{icon_button, muted, text_button};

impl EnvApp {
    pub(crate) fn render_project(&self, _: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(unlocked) = self.vault() else {
            return div().into_any_element();
        };
        let Some(project) = unlocked.project() else {
            return empty_state(
                "No project selected",
                "Create a project, or open a folder from the project strip (⌥⌘O)",
            )
            .into_any_element();
        };
        let colors = cx.theme().colors().clone();
        let key_badge = match &project.share_key {
            Some(key) => format!("Share key {}", key.fingerprint()),
            None => "No share key".to_string(),
        };
        let header = h_flex()
            .flex_none()
            .px_4()
            .pt_3()
            .pb_2()
            .gap_2()
            .justify_between()
            .child(
                v_flex()
                    .min_w_0()
                    .child(Headline::new(project.name.clone()).size(HeadlineSize::Small))
                    .child(muted(project.path.as_ref().map_or_else(
                        || "Not linked to a folder".to_string(),
                        |p| p.display().to_string(),
                    ))),
            )
            .child(
                h_flex()
                    .gap_1()
                    .child(
                        text_button("share-key", key_badge, cx, |this, window, cx| {
                            this.open_share_key(window, cx);
                        })
                        .start_icon(Icon::new(IconName::Lock).size(IconSize::XSmall)),
                    )
                    .child(
                        text_button("share", "Share…", cx, |this, window, cx| {
                            this.open_share(window, cx);
                        })
                        .start_icon(Icon::new(IconName::Share).size(IconSize::XSmall)),
                    )
                    .child(icon_button(
                        "delete-project",
                        IconName::Trash,
                        "Delete project",
                        cx,
                        |this, window, cx| {
                            if let Some(index) = this.vault().and_then(|u| u.project) {
                                this.confirm(ConfirmAction::DeleteProject(index), window, cx);
                            }
                        },
                    )),
            );

        let mut tabs = TabBar::new("env-tabs");
        for (index, env) in project.environments.iter().enumerate() {
            tabs = tabs.child(
                Tab::new(SharedString::from(format!("env-{index}")))
                    .toggle_state(index == unlocked.env)
                    .child(Label::new(env.name.clone()).size(LabelSize::Small))
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.select_environment(index, cx)),
                    ),
            );
        }
        tabs = tabs.end_child(icon_button(
            "new-env",
            IconName::Plus,
            "New environment",
            cx,
            |this, window, cx| this.open_environment_name(None, window, cx),
        ));

        v_flex()
            .size_full()
            .child(header)
            .child(tabs)
            .child(self.render_env_toolbar(cx))
            .child(
                div()
                    .id("variables")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .border_t_1()
                    .border_color(colors.border_variant)
                    .child(self.render_variables(cx)),
            )
            .into_any_element()
    }

    fn render_env_toolbar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let reveal_all = self.vault().is_some_and(|u| u.reveal_all);
        let has_env = self.vault().and_then(|u| u.environment()).is_some();
        let row = h_flex().flex_none().px_3().py_1().gap_1().flex_wrap();
        if !has_env {
            return row.child(muted("No environments. Add one with +."));
        }
        row.child(
            Button::new("add-var", "Add variable")
                .style(ButtonStyle::Filled)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .start_icon(Icon::new(IconName::Plus).size(IconSize::XSmall))
                .tooltip(|_, cx| Tooltip::for_action("Add variable", &NewVariable, cx))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_variable_dialog(None, window, cx);
                })),
        )
        .child(text_button(
            "reveal-all",
            if reveal_all { "Hide all" } else { "Reveal all" },
            cx,
            |this, _, cx| {
                if let Some(u) = this.vault_mut() {
                    u.reveal_all = !u.reveal_all;
                    u.revealed.clear();
                }
                cx.notify();
            },
        ))
        .child(text_button(
            "import-env",
            "Import .env…",
            cx,
            |this, window, cx| {
                this.open_import_dotenv(window, cx);
            },
        ))
        .child(text_button(
            "export-env",
            "Write .env…",
            cx,
            |this, window, cx| {
                this.open_export_dotenv(window, cx);
            },
        ))
        .child(text_button(
            "copy-env",
            "Copy as .env",
            cx,
            |this, _, cx| {
                let text = this
                    .vault()
                    .and_then(|u| u.environment())
                    .map(|e| dotenv::to_dotenv(&e.variables));
                if let Some(text) = text {
                    this.copy_secret(&text, ".env text", cx);
                }
            },
        ))
        .child(text_button(
            "compare",
            "Compare…",
            cx,
            |this, window, cx| {
                this.open_compare(window, cx);
            },
        ))
        .child(div().flex_1())
        .child(text_button(
            "rename-env",
            "Rename",
            cx,
            |this, window, cx| {
                let index = this.vault().map(|u| u.env);
                this.open_environment_name(index, window, cx);
            },
        ))
        .child(icon_button(
            "delete-env",
            IconName::Trash,
            "Delete environment",
            cx,
            |this, window, cx| {
                if let Some(index) = this.vault().map(|u| u.env) {
                    this.confirm(ConfirmAction::DeleteEnvironment(index), window, cx);
                }
            },
        ))
    }

    fn render_variables(&self, cx: &mut Context<Self>) -> AnyElement {
        let Some(unlocked) = self.vault() else {
            return div().into_any_element();
        };
        let Some(env) = unlocked.environment() else {
            return div().into_any_element();
        };
        if env.variables.is_empty() {
            return div()
                .h(px(200.))
                .child(empty_state(
                    format!("{} has no variables", env.name),
                    "Add one (⌘N) or import a .env file",
                ))
                .into_any_element();
        }
        let colors = cx.theme().colors().clone();
        let mut table = v_flex().w_full();
        for (index, var) in env.variables.iter().enumerate() {
            let shown = !var.secret || unlocked.reveal_all || unlocked.revealed.contains(&var.key);
            let value: SharedString = if shown {
                preview(&var.value).into()
            } else {
                MASK.into()
            };
            let key = var.key.clone();
            table = table.child(
                h_flex()
                    .id(SharedString::from(format!("var-{index}")))
                    .w_full()
                    .px_4()
                    .py_1p5()
                    .gap_3()
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .hover(|s| s.bg(colors.element_hover))
                    .on_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
                        if event.click_count() >= 2 {
                            this.open_variable_dialog(Some(index), window, cx);
                        }
                    }))
                    .child(
                        div()
                            .w(px(220.))
                            .flex_none()
                            .overflow_hidden()
                            .font_buffer(cx)
                            .text_ui_sm(cx)
                            .child(var.key.clone()),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                div()
                                    .font_buffer(cx)
                                    .text_ui_sm(cx)
                                    .text_color(if shown {
                                        colors.text
                                    } else {
                                        colors.text_muted
                                    })
                                    .overflow_hidden()
                                    .child(value),
                            )
                            .when(!var.comment.is_empty(), |col| {
                                col.child(muted(var.comment.clone()).size(LabelSize::XSmall))
                            }),
                    )
                    .child(self.row_actions(index, key, shown, var.secret, cx)),
            );
        }
        table.into_any_element()
    }

    fn row_actions(
        &self,
        index: usize,
        key: String,
        shown: bool,
        secret: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let reveal_key = key.clone();
        h_flex()
            .flex_none()
            .gap_0p5()
            .when(secret, |row| {
                row.child(icon_button(
                    SharedString::from(format!("reveal-{index}")),
                    if shown {
                        IconName::EyeOff
                    } else {
                        IconName::Eye
                    },
                    if shown { "Hide value" } else { "Reveal value" },
                    cx,
                    move |this, _, cx| {
                        if let Some(u) = this.vault_mut()
                            && !u.revealed.remove(&reveal_key)
                        {
                            u.revealed.insert(reveal_key.clone());
                        }
                        cx.notify();
                    },
                ))
            })
            .child(icon_button(
                SharedString::from(format!("copy-{index}")),
                IconName::Copy,
                "Copy value (clears in 30 s)",
                cx,
                move |this, _, cx| this.copy_variable(index, cx),
            ))
            .child(icon_button(
                SharedString::from(format!("edit-{index}")),
                IconName::Pencil,
                "Edit",
                cx,
                move |this, window, cx| this.open_variable_dialog(Some(index), window, cx),
            ))
            .child(icon_button(
                SharedString::from(format!("delete-{index}")),
                IconName::Trash,
                "Delete",
                cx,
                move |this, window, cx| {
                    this.confirm(ConfirmAction::DeleteVariable(index), window, cx);
                },
            ))
    }

    pub(crate) fn copy_variable(&mut self, index: usize, cx: &mut Context<Self>) {
        let value = self
            .vault()
            .and_then(|u| u.environment())
            .and_then(|e| e.variables.get(index))
            .map(|v| (zeroize::Zeroizing::new(v.value.clone()), v.key.clone()));
        if let Some((value, key)) = value {
            self.copy_secret(&value, &key, cx);
        }
    }
}

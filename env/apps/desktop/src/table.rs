//! The selected project's card: a header row with the environment tabs and
//! the environment / project actions, then the variable table (values
//! masked until revealed).

use ferrum_ui::components::Clickable;
use ferrum_ui::prelude::*;
use ferrumenv_core::dotenv;

use crate::app::{EnvApp, NewVariable};
use crate::dialogs::ConfirmAction;
use crate::secrets::{MASK, preview};
use crate::widgets::{icon_button, muted};

impl EnvApp {
    /// The card's body: header row, then the variables (or an empty state).
    pub(crate) fn render_project(&self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(unlocked) = self.vault() else {
            return div().into_any_element();
        };
        let header = self.render_card_header(window, cx);
        if unlocked.project().is_none() {
            return v_flex()
                .size_full()
                .child(header)
                .child(empty_state(
                    "No project selected",
                    "Create a project with +, or open a folder (⌥⌘O)",
                ))
                .into_any_element();
        }
        v_flex()
            .size_full()
            .child(header)
            .child(
                div()
                    .id("variables")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .child(self.render_variables(cx)),
            )
            .into_any_element()
    }

    /// The card's 40px header: (window controls and project while the
    /// sidebar is hidden,) environment tabs, then the actions as icons.
    fn render_card_header(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let sidebar_open = self.sidebar_open;
        let mut row = ferrum_ui::title_row("pane-header", !sidebar_open, window)
            .gap_1()
            .border_b_1()
            .border_color(colors.border_variant)
            .text_ui_sm(cx);
        // The Ferrum account (Sign in / avatar) closes the row, at the
        // window's top right.
        let account = |row: ferrum_ui::gpui::Stateful<ferrum_ui::gpui::Div>,
                       cx: &mut Context<Self>| {
            row.child(div().flex_1())
                .children(ferrum_account_ui::account_button(cx))
        };
        let Some(unlocked) = self.vault() else {
            return account(row, cx);
        };
        if !sidebar_open {
            row = row
                .child(self.title_controls(cx))
                .child(
                    div()
                        .flex_none()
                        .max_w(px(260.))
                        .child(unlocked.bar.clone()),
                )
                .child(div().w(px(6.)));
        }
        let Some(project) = unlocked.project() else {
            return account(row, cx);
        };
        let mut tabs = h_flex()
            .id("env-tabs")
            .flex_1()
            .min_w_0()
            .gap_1()
            .overflow_x_scroll();
        for (index, env) in project.environments.iter().enumerate() {
            tabs = tabs.child(
                ferrum_ui::header_tab(
                    SharedString::from(format!("env-{index}")),
                    index == unlocked.env,
                    cx,
                )
                .child(env.name.clone())
                .on_click(cx.listener(move |this, _, _, cx| this.select_environment(index, cx))),
            );
        }
        tabs = tabs.child(
            ferrum_ui::title_button("new-env", IconName::Plus, "New environment").on_click(
                cx.listener(|this, _, window, cx| this.open_environment_name(None, window, cx)),
            ),
        );
        let key_tooltip = match &project.share_key {
            Some(key) => format!("Share key {}", key.fingerprint()),
            None => "No share key: set one up".to_string(),
        };
        let team_tooltip = match project.members.len() {
            0 => "Team keys: add teammates' public keys".to_string(),
            n => format!("Team keys ({n} members)"),
        };
        row.child(tabs)
            .when(unlocked.environment().is_some(), |row| {
                row.child(self.env_actions(cx))
            })
            .child(div().w(px(1.)).h(px(16.)).mx_1().bg(colors.border_variant))
            .child(icon_button(
                "team",
                IconName::Person,
                team_tooltip,
                cx,
                |this, window, cx| this.open_team(window, cx),
            ))
            .child(icon_button(
                "sync",
                if project.sync.is_some() {
                    IconName::Cloud
                } else {
                    IconName::RotateCw
                },
                match &project.sync {
                    Some(state) => format!("Synced through {}", state.folder.display()),
                    None => "Sync through a shared folder…".to_string(),
                },
                cx,
                |this, window, cx| this.open_sync(window, cx),
            ))
            .child(icon_button(
                "leak-scan",
                IconName::Warning,
                "Scan the folder and git history for leaked secrets…",
                cx,
                |this, window, cx| this.open_leak_scan(window, cx),
            ))
            .child(icon_button(
                "share-key",
                IconName::Hash,
                key_tooltip,
                cx,
                |this, window, cx| this.open_share_key(window, cx),
            ))
            .child(icon_button(
                "share",
                IconName::Share,
                "Share…",
                cx,
                |this, window, cx| this.open_share(window, cx),
            ))
            .when(!sidebar_open, |row| {
                row.child(icon_button(
                    "lock-now-header",
                    IconName::Lock,
                    "Lock now (⌘L)",
                    cx,
                    |this, window, cx| this.lock(window, cx),
                ))
            })
            .children(ferrum_account_ui::account_button(cx))
    }

    /// Actions on the current environment, as small icon buttons.
    fn env_actions(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let reveal_all = self.vault().is_some_and(|u| u.reveal_all);
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                IconButton::new("add-var", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .tooltip(|_, cx| Tooltip::for_action("Add variable", &NewVariable, cx))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_variable_dialog(None, window, cx);
                    })),
            )
            .child(icon_button(
                "reveal-all",
                if reveal_all {
                    IconName::EyeOff
                } else {
                    IconName::Eye
                },
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
            .child(icon_button(
                "import-env",
                IconName::Download,
                "Import .env…",
                cx,
                |this, window, cx| this.open_import_dotenv(window, cx),
            ))
            .child(icon_button(
                "export-env",
                IconName::File,
                "Write .env…",
                cx,
                |this, window, cx| this.open_export_dotenv(window, cx),
            ))
            .child(icon_button(
                "copy-env",
                IconName::Copy,
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
            .child(icon_button(
                "compare",
                IconName::ArrowRightLeft,
                "Compare…",
                cx,
                |this, window, cx| this.open_compare(window, cx),
            ))
            .child(icon_button(
                "rename-env",
                IconName::Pencil,
                "Rename environment",
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

//! Window chrome: the ◧ ⌘ + controls of the title row and the compact
//! Run / Cancel / Theme tools at the end of the top card's header.

use crate::app::FerrumData;
use crate::components::env_dot;
use crate::state::Dialog;
use ferrum_ui::components::Disableable as _;
use ferrum_ui::prelude::*;
use ferrumdata_domain::{ProfileId, Theme};

impl FerrumData {
    /// ◧ (sidebar), ⌘ (the tab's connection) and + (new connection): in
    /// the sidebar's title row, or first in the top card's header while the
    /// sidebar is hidden.
    pub fn render_title_controls(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let open = !self.state.sidebar_hidden;
        h_flex()
            .flex_none()
            .gap_0p5()
            .child(
                ferrum_ui::sidebar_toggle("toggle-sidebar", open)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_sidebar(cx))),
            )
            .child(self.connection_menu(cx))
            .child(
                ferrum_ui::title_button("title-new-connection", IconName::Plus, "New connection")
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.open_dialog(Dialog::ConnectionForm(None), window, cx);
                    })),
            )
            .into_any_element()
    }

    /// ⌘: picks the connection of the active tab.
    fn connection_menu(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let has_tab = self.state.active_tab().is_some();
        let current = self.state.active_profile().map(|p| p.id);
        let this = cx.entity().downgrade();
        let choices: Vec<(Option<ProfileId>, String)> =
            std::iter::once((None, "No connection".to_owned()))
                .chain(
                    self.state
                        .profiles
                        .iter()
                        .map(|p| (Some(p.id), p.display_name.clone())),
                )
                .collect();
        let build = move |window: &mut Window, cx: &mut App| {
            let this = this.clone();
            let choices = choices.clone();
            ContextMenu::build(window, cx, move |mut menu, _, _| {
                menu = menu.header("Connection for this tab");
                for (id, label) in choices {
                    let this = this.clone();
                    menu = menu.toggleable_entry(
                        label,
                        id == current,
                        IconPosition::Start,
                        None,
                        move |_, cx| {
                            if let Err(e) = this.update(cx, |this, cx| {
                                this.state.bind_active_tab(id);
                                cx.notify();
                            }) {
                                tracing::debug!(error = %e, "header dropped");
                            }
                        },
                    );
                }
                menu
            })
        };
        PopoverMenu::new("connection-picker")
            .menu(move |window, cx| Some(build(window, cx)))
            .trigger(
                ferrum_ui::command_button("connection-picker-button", "Connection for this tab")
                    .disabled(!has_tab),
            )
            .into_any_element()
    }

    /// Production / read-only flags of the tab's connection, then Run,
    /// Cancel and the theme switch.
    pub fn render_header_tools(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let profile = self.state.active_profile().cloned();
        let running = self.state.active_run_in_flight();
        let theme = self.state.settings.theme;
        let theme_tip = match theme {
            Theme::System => "Theme: System (click for Light)",
            Theme::Light => "Theme: Light (click for Dark)",
            Theme::Dark => "Theme: Dark (click for System)",
        };
        let status = cx.theme().status().clone();

        h_flex()
            .flex_none()
            .gap_1()
            .when_some(profile.as_ref(), |this, p| {
                this.child(env_dot(p.env, 8., cx))
                    .when(p.env.is_production(), |this| {
                        this.child(badge("PRODUCTION", status.error, cx))
                    })
                    .when(p.read_only, |this| {
                        this.child(badge("Read only", status.info, cx))
                    })
            })
            .child(
                ferrum_ui::title_button(
                    "run",
                    IconName::PlayFilled,
                    "Run selection or current statement (⌘/Ctrl+Enter)",
                )
                .icon_color(if running || profile.is_none() {
                    Color::Disabled
                } else {
                    Color::Success
                })
                .disabled(running || profile.is_none())
                .on_click(cx.listener(|this, _, _, cx| this.run_from_editor(cx))),
            )
            .child(
                ferrum_ui::title_button("cancel", IconName::Stop, "Cancel the running query")
                    .icon_color(if running {
                        Color::Error
                    } else {
                        Color::Disabled
                    })
                    .disabled(!running)
                    .on_click(cx.listener(|this, _, _, _| this.cancel_active())),
            )
            .child(
                ferrum_ui::title_button("theme", IconName::Monitor, theme_tip).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.state.settings.theme = match theme {
                            Theme::System => Theme::Light,
                            Theme::Light => Theme::Dark,
                            Theme::Dark => Theme::System,
                        };
                        this.save_settings(cx);
                        cx.notify();
                    }),
                ),
            )
            .into_any_element()
    }

    pub fn toggle_sidebar(&mut self, cx: &mut Context<Self>) {
        self.state.sidebar_hidden = !self.state.sidebar_hidden;
        cx.notify();
    }
}

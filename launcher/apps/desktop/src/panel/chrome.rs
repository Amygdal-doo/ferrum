//! The panel's frame: the header (logo, ⚙ menu), the tabs with the search
//! field, and the toast at the bottom.

use ferrum_ui::components::{ButtonSize, TintColor};
use ferrum_ui::gpui::{Anchor, FontWeight};
use ferrum_ui::prelude::*;
use ferrumlauncher_core::AppId;

use super::{LauncherPanel, Tab, projects};

impl LauncherPanel {
    pub(super) fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let launcher = self.launcher.read(cx);
        let logo = launcher.logo(AppId::LAUNCHER, px(30.));
        let updates = launcher.state.app_updates().len();
        let launcher_update = launcher.state.update_for(AppId::LAUNCHER).is_some();
        let accent = cx.theme().colors().text_accent;
        let weak = cx.weak_entity();
        let settings = PopoverMenu::new("panel-settings")
            .anchor(Anchor::TopRight)
            .trigger(
                IconButton::new("panel-settings-button", IconName::Settings)
                    .tooltip(Tooltip::text("Ferrum Launcher")),
            )
            .menu(move |window, cx| {
                let weak = weak.clone();
                Some(ContextMenu::build(window, cx, move |menu, _, _| {
                    let (open, check, all, settings) =
                        (weak.clone(), weak.clone(), weak.clone(), weak.clone());
                    let menu = menu
                        .icon_entry(
                            "Open Ferrum Launcher",
                            IconName::ArrowUpRight,
                            move |window, cx| {
                                open.update(cx, |this, cx| this.show_main(None, false, window, cx))
                                    .ok();
                            },
                        )
                        .icon_entry("Check for Updates", IconName::RotateCw, move |_, cx| {
                            check
                                .update(cx, |this, cx| {
                                    this.launcher
                                        .update(cx, |l, cx| l.check_for_updates(true, cx));
                                })
                                .ok();
                        });
                    let menu = if updates > 0 {
                        menu.icon_entry(
                            format!("Update All ({updates})"),
                            IconName::Download,
                            move |_, cx| {
                                all.update(cx, |this, cx| {
                                    this.launcher.update(cx, |l, cx| l.update_all(cx));
                                })
                                .ok();
                            },
                        )
                    } else {
                        menu.disabled_entry("Every app is up to date")
                    };
                    menu.separator()
                        .icon_entry("Settings…", IconName::Settings, move |window, cx| {
                            settings
                                .update(cx, |this, cx| this.show_main(None, true, window, cx))
                                .ok();
                        })
                        .entry("Quit Ferrum Launcher", None, |_, cx| cx.quit())
                }))
            });
        h_flex()
            .flex_none()
            .px_2()
            .py_1p5()
            .gap_2()
            .child(logo)
            .child(
                v_flex()
                    .flex_1()
                    .child(Label::new("Ferrum").weight(FontWeight::BOLD))
                    .child(
                        Label::new("Launcher")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
            .child(
                div()
                    .relative()
                    .child(settings)
                    .when(launcher_update, |this| {
                        this.child(
                            div()
                                .absolute()
                                .top(px(2.))
                                .right(px(2.))
                                .size(px(7.))
                                .rounded_full()
                                .bg(accent),
                        )
                    }),
            )
    }

    pub(super) fn render_tabs(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let installed = {
            let state = &self.launcher.read(cx).state;
            state
                .managed_apps()
                .iter()
                .filter(|info| state.active(info.id).is_some())
                .count()
        };
        let tab = |id: &'static str, label: &'static str, count: Option<usize>, tab: Tab| {
            let active = self.tab == tab;
            h_flex()
                .id(id)
                .flex_none()
                .h_full()
                .gap_1()
                .px_1()
                .border_b_2()
                .border_color(if active {
                    colors.text_accent
                } else {
                    ferrum_ui::gpui::transparent_black()
                })
                .cursor_pointer()
                .child(
                    Label::new(label)
                        .size(LabelSize::Small)
                        .weight(FontWeight::MEDIUM)
                        .color(if active { Color::Default } else { Color::Muted }),
                )
                .when_some(count, |this, count| {
                    this.child(
                        Label::new(count.to_string())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.tab = tab;
                    if tab == Tab::Projects {
                        this.projects = projects::load();
                    }
                    cx.notify();
                }))
        };
        let has_query = !self.search.read(cx).is_empty();
        ferrum_ui::pane_frame(cx).flex_none().child(
            h_flex()
                .h(px(34.))
                .px_2()
                .gap_3()
                .child(tab("panel-tab-apps", "Apps", Some(installed), Tab::Apps))
                .child(tab("panel-tab-projects", "Projects", None, Tab::Projects))
                .child(
                    h_flex()
                        .flex_1()
                        .min_w_0()
                        .gap_1p5()
                        .pl_2()
                        .child(
                            Icon::new(IconName::MagnifyingGlass)
                                .size(IconSize::Small)
                                .color(Color::Muted),
                        )
                        .child(div().flex_1().min_w_0().child(self.search.clone()))
                        .when(has_query, |this| {
                            this.child(
                                IconButton::new("panel-search-clear", IconName::Close)
                                    .icon_size(IconSize::Small)
                                    .tooltip(Tooltip::text("Clear the search"))
                                    .on_click(cx.listener(|this, _, window, cx| {
                                        this.search.update(cx, |input, cx| {
                                            input.set_text("", cx);
                                            input.focus(window, cx);
                                        });
                                        cx.notify();
                                    })),
                            )
                        }),
                ),
        )
    }

    /// The launcher's own update, or its last message.
    pub(super) fn render_toast(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let colors = cx.theme().colors().clone();
        let launcher = self.launcher.read(cx);
        let toast = h_flex()
            .flex_none()
            .px_3()
            .py_1p5()
            .gap_2()
            .rounded(px(10.))
            .border_1()
            .bg(colors.elevated_surface_background);
        if let Some(notice) = &launcher.notice {
            let error = notice.tone == NoticeTone::Error;
            return Some(
                toast
                    .border_color(colors.border_variant)
                    .child(
                        Icon::new(if error {
                            IconName::Warning
                        } else {
                            IconName::Info
                        })
                        .size(IconSize::Small)
                        .color(if error {
                            Color::Error
                        } else {
                            Color::Muted
                        }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .child(Label::new(notice.message.clone()).size(LabelSize::Small)),
                    )
                    .into_any_element(),
            );
        }
        let release = launcher.state.update_for(AppId::LAUNCHER)?;
        let version = release.version.clone();
        if self.dismissed_update.as_ref() == Some(&version) {
            return None;
        }
        let url = release.html_url.clone();
        let self_updatable = launcher.running_bundle.is_some();
        let updating = launcher.self_update.is_some();
        let dismissed = version.clone();
        Some(
            toast
                .border_color(colors.text_accent.opacity(0.5))
                .child(
                    div().flex_1().min_w_0().child(
                        Label::new(if updating {
                            format!("Updating the launcher to {version}…")
                        } else {
                            format!("Launcher update {version} is ready")
                        })
                        .size(LabelSize::Small)
                        .truncate(),
                    ),
                )
                .child(
                    Button::new("panel-launcher-update", "Update")
                        .size(ButtonSize::Compact)
                        .style(ButtonStyle::Tinted(TintColor::Accent))
                        .disabled(updating)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if self_updatable {
                                let version = version.clone();
                                this.launcher
                                    .update(cx, |l, cx| l.update_launcher(version, cx));
                            } else {
                                cx.open_url(&url);
                            }
                        })),
                )
                .child(
                    IconButton::new("panel-launcher-update-dismiss", IconName::Close)
                        .icon_size(IconSize::Small)
                        .tooltip(Tooltip::text("Dismiss"))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.dismissed_update = Some(dismissed.clone());
                            cx.notify();
                        })),
                )
                .into_any_element(),
        )
    }
}

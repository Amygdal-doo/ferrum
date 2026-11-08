use crate::app::FerrumData;
use crate::components::env_dot;
use ferrum_ui::prelude::*;
use ferrum_ui::ui::TintColor;
use ferrumdata_domain::{ProfileId, Theme};

impl FerrumData {
    pub fn render_header(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let has_tab = self.state.active_tab().is_some();
        let profile = self.state.active_profile().cloned();
        let running = self.state.active_run_in_flight();
        let theme = self.state.settings.theme;
        let theme_label = match theme {
            Theme::System => "Theme: System",
            Theme::Light => "Theme: Light",
            Theme::Dark => "Theme: Dark",
        };
        let status = cx.theme().status().clone();

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
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for (id, label) in choices {
                let this = this.clone();
                menu = menu.entry(label, None, move |_, cx| {
                    if let Err(e) = this.update(cx, |this, cx| {
                        this.state.bind_active_tab(id);
                        cx.notify();
                    }) {
                        tracing::debug!(error = %e, "header dropped");
                    }
                });
            }
            menu
        });
        let selected = profile
            .as_ref()
            .map_or_else(|| "No connection".to_owned(), |p| p.display_name.clone());

        toolbar(cx)
            .h(px(48.))
            .child(Label::new("FerrumData").weight(ferrum_ui::gpui::FontWeight::SEMIBOLD))
            .child(Divider::vertical())
            .when_some(profile.as_ref(), |this, p| {
                this.child(env_dot(p.env, 10., cx))
            })
            .child(
                div().min_w(px(224.)).child(
                    DropdownMenu::new("connection-picker", selected, menu)
                        .full_width(true)
                        .disabled(!has_tab)
                        .aria_label("Connection for this tab"),
                ),
            )
            .when_some(profile.as_ref(), |this, p| {
                this.when(p.env.is_production(), |this| {
                    this.child(badge("PRODUCTION", status.error, cx))
                })
                .when(p.read_only, |this| {
                    this.child(badge("Read only", status.info, cx))
                })
            })
            .child(div().flex_1())
            .child(
                Button::new("run", "Run ▷")
                    .style(ButtonStyle::Filled)
                    .disabled(running || profile.is_none())
                    .tooltip(Tooltip::text(
                        "Run selection or current statement (⌘/Ctrl+Enter)",
                    ))
                    .on_click(cx.listener(|this, _, _, cx| this.run_from_editor(cx))),
            )
            .child(kbd("⌘↵", cx))
            .child(
                Button::new("cancel", "Cancel ■")
                    .style(ButtonStyle::Tinted(TintColor::Error))
                    .disabled(!running)
                    .tooltip(Tooltip::text("Cancel the running query"))
                    .on_click(cx.listener(|this, _, _, _| this.cancel_active())),
            )
            .child(
                Button::new("theme", theme_label)
                    .tooltip(Tooltip::text("Switch theme"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.settings.theme = match theme {
                            Theme::System => Theme::Light,
                            Theme::Light => Theme::Dark,
                            Theme::Dark => Theme::System,
                        };
                        this.save_settings(cx);
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}

//! Releases: every published release across all apps, newest first.

use super::logo::logo;
use super::{channel_badge, format_count, format_date, format_size};
use crate::app::FerrumToolbox;
use ferrum_ui::prelude::*;
use ferrumtoolbox_core::{AppId, Channel, ReleaseInfo, current_target};

const CHANNELS: [Channel; 3] = [Channel::Stable, Channel::Beta, Channel::Alpha];

impl FerrumToolbox {
    pub fn render_releases(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let releases: Vec<ReleaseInfo> = self
            .state
            .timeline()
            .into_iter()
            .filter(|release| release.app != AppId::Toolbox)
            .filter(|release| self.timeline_app.is_none_or(|app| release.app == app))
            .filter(|release| {
                self.timeline_channel
                    .is_none_or(|channel| release.channel() == channel)
            })
            .cloned()
            .collect();
        let total = self
            .state
            .releases
            .iter()
            .filter(|release| release.app != AppId::Toolbox)
            .count();
        let empty = releases.is_empty();
        let rows: Vec<AnyElement> = releases
            .into_iter()
            .map(|release| self.render_release_row(release, cx))
            .collect();

        let app_filters: Vec<AnyElement> = std::iter::once(None)
            .chain(AppId::MANAGED.into_iter().map(Some))
            .map(|app| {
                let label = app.map_or("All apps", |app| app.info().name);
                let id = format!("filter-app-{}", app.map_or("all", AppId::as_str));
                Button::new(SharedString::from(id), label)
                    .style(ButtonStyle::Subtle)
                    .toggle_state(self.timeline_app == app)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.timeline_app = app;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let channel_filters: Vec<AnyElement> = std::iter::once(None)
            .chain(CHANNELS.into_iter().map(Some))
            .map(|channel| {
                let label = channel.map_or("All channels", Channel::label);
                let id = format!("filter-channel-{}", channel.map_or("all", Channel::label));
                Button::new(SharedString::from(id), label)
                    .style(ButtonStyle::Subtle)
                    .toggle_state(self.timeline_channel == channel)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.timeline_channel = channel;
                        cx.notify();
                    }))
                    .into_any_element()
            })
            .collect();
        let colors = cx.theme().colors();

        v_flex()
            .size_full()
            .child(
                v_flex()
                    .flex_none()
                    .gap_2()
                    .p_4()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(Headline::new("Releases").size(HeadlineSize::Small))
                    .child(
                        Label::new(format!(
                            "{total} release{} across all apps, newest first.",
                            if total == 1 { "" } else { "s" }
                        ))
                        .color(Color::Muted),
                    )
                    .child(h_flex().flex_wrap().gap_1().children(app_filters))
                    .child(h_flex().flex_wrap().gap_1().children(channel_filters)),
            )
            .child(
                v_flex()
                    .id("timeline")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .when(empty, |this| {
                        this.child(empty_state(
                            if total == 0 {
                                "No releases yet"
                            } else {
                                "No releases match"
                            },
                            if total == 0 {
                                "Check for updates, or publish a release tagged <app>-v<version>."
                            } else {
                                "Try another app or channel filter."
                            },
                        ))
                    })
                    .children(rows),
            )
    }

    fn render_release_row(&self, release: ReleaseInfo, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let app = release.app;
        let tag = release.tag.clone();
        let expanded = self.expanded_notes.contains(&tag);
        let size = current_target()
            .and_then(|target| release.assets_for(target))
            .map(|(asset, _)| format_size(asset.size))
            .unwrap_or_else(|| "No build for this OS".into());
        let installed = self.state.app(app).is_installed(&release.version);
        let active = self.state.active(app) == Some(&release.version);
        let notes = if release.notes.trim().is_empty() {
            "No release notes.".to_string()
        } else {
            release.notes.clone()
        };
        let url = Some(release.html_url.clone()).filter(|url| !url.is_empty());

        v_flex()
            .border_b_1()
            .border_color(colors.border_variant)
            .child(
                h_flex()
                    .id(SharedString::from(format!("release-{tag}")))
                    .px_4()
                    .py_2()
                    .gap_3()
                    .cursor_pointer()
                    .hover(|style| style.bg(colors.ghost_element_hover))
                    .child(
                        Icon::new(if expanded {
                            IconName::ChevronDown
                        } else {
                            IconName::ChevronRight
                        })
                        .size(IconSize::Small)
                        .color(Color::Muted),
                    )
                    .child(logo(app, px(28.)))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Label::new(format!(
                                        "{} {}",
                                        app.info().name,
                                        release.version
                                    )))
                                    .child(channel_badge(release.channel(), cx))
                                    .when(active, |this| {
                                        this.child(badge("active", cx.theme().status().success, cx))
                                    })
                                    .when(installed && !active, |this| {
                                        this.child(badge(
                                            "installed",
                                            cx.theme().colors().text_muted,
                                            cx,
                                        ))
                                    }),
                            )
                            .child(
                                Label::new(release.title.clone())
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                    )
                    .child(
                        Label::new(format!(
                            "{} · {size} · {} downloads",
                            format_date(release.published_at),
                            format_count(release.downloads())
                        ))
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    )
                    .on_click(cx.listener({
                        let tag = tag.clone();
                        move |this, _, _, cx| {
                            if !this.expanded_notes.remove(&tag) {
                                this.expanded_notes.insert(tag.clone());
                            }
                            cx.notify();
                        }
                    })),
            )
            .when(expanded, |this| {
                this.child(
                    v_flex()
                        .gap_2()
                        .pl(px(72.))
                        .pr_4()
                        .pb_3()
                        .child(div().w_full().text_sm().child(notes))
                        .child(
                            h_flex()
                                .gap_2()
                                .child(
                                    Button::new(
                                        SharedString::from(format!("release-app-{tag}")),
                                        format!("Open {}", app.info().name),
                                    )
                                    .style(ButtonStyle::Subtle)
                                    .on_click(
                                        cx.listener(move |this, _, _, cx| this.select_app(app, cx)),
                                    ),
                                )
                                .when_some(url, |this, url| {
                                    this.child(
                                        Button::new(
                                            SharedString::from(format!("release-gh-{tag}")),
                                            "Open on GitHub",
                                        )
                                        .style(ButtonStyle::Subtle)
                                        .start_icon(Icon::new(IconName::ArrowUpRight))
                                        .on_click(move |_, _, cx| cx.open_url(&url)),
                                    )
                                }),
                        ),
                )
            })
            .into_any_element()
    }
}

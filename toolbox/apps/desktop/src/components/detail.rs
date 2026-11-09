//! Selected app: actions, versions and release notes.

use super::logo::logo;
use super::{channel_badge, format_count, format_size, status_badge};
use crate::app::{FerrumToolbox, Page};
use ferrum_ui::prelude::*;
use ferrumtoolbox_core::{AppId, ReleaseInfo, UpdateChannel, current_target};
use semver::Version;

/// One row of the versions list: a published release, an installed version
/// whose release is gone, or both.
struct VersionRow {
    version: Version,
    release: Option<ReleaseInfo>,
    /// Installed by this toolbox (so it can be activated or removed).
    installed: bool,
    active: bool,
    /// The toolbox binary showing this window.
    running: bool,
}

impl FerrumToolbox {
    fn version_rows(&self, app: AppId) -> Vec<VersionRow> {
        let app_state = self.state.app(app);
        let active = self.state.active(app);
        let mut rows: Vec<VersionRow> = self
            .state
            .releases_for(app)
            .into_iter()
            .map(|release| VersionRow {
                version: release.version.clone(),
                installed: app_state.is_installed(&release.version),
                active: active == Some(&release.version),
                running: self.is_running(app, &release.version),
                release: Some(release.clone()),
            })
            .collect();
        for installed in &app_state.installed {
            if !rows.iter().any(|row| row.version == installed.version) {
                rows.push(VersionRow {
                    version: installed.version.clone(),
                    release: None,
                    installed: true,
                    active: active == Some(&installed.version),
                    running: self.is_running(app, &installed.version),
                });
            }
        }
        rows.sort_by(|a, b| b.version.cmp(&a.version));
        rows
    }

    fn is_running(&self, app: AppId, version: &Version) -> bool {
        app == AppId::Toolbox && self.state.running_toolbox.as_ref() == Some(version)
    }

    pub fn render_detail(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app = self.selected;
        let border = cx.theme().colors().border;
        let active = self.state.active(app).cloned();
        let latest = self.state.latest(app).cloned();
        let rows = self.version_rows(app);
        let downloading = self.downloads.contains_key(&app);

        let notes_release = self
            .selected_release
            .as_ref()
            .and_then(|v| rows.iter().find(|row| &row.version == v))
            .and_then(|row| row.release.clone())
            .or_else(|| latest.clone());
        let installed_notes = self
            .selected_release
            .as_ref()
            .or(active.as_ref())
            .and_then(|v| {
                self.state
                    .app(app)
                    .installed
                    .into_iter()
                    .find(|installed| &installed.version == v)
            })
            .map(|installed| (installed.version.clone(), installed.notes));

        let actions = self.render_actions(app, active.clone(), latest.clone(), cx);
        let download = downloading.then(|| self.render_download(app, cx));
        let version_rows: Vec<AnyElement> = rows
            .into_iter()
            .map(|row| self.render_version_row(app, row, downloading, cx))
            .collect();
        let rows_empty = version_rows.is_empty();

        v_flex()
            .size_full()
            .child(
                v_flex()
                    .flex_none()
                    .gap_3()
                    .p_4()
                    .border_b_1()
                    .border_color(border)
                    .child(
                        h_flex().child(
                            Button::new("back-to-overview", "All apps")
                                .style(ButtonStyle::Subtle)
                                .start_icon(Icon::new(IconName::ArrowLeft))
                                .on_click(
                                    cx.listener(|this, _, _, cx| {
                                        this.show_page(Page::Overview, cx)
                                    }),
                                ),
                        ),
                    )
                    .child(self.render_header(app, cx))
                    .child(actions)
                    .children(download),
            )
            .child(
                h_flex()
                    .flex_1()
                    .min_h_0()
                    .items_start()
                    .child(
                        v_flex()
                            .id("versions")
                            .w(relative(0.55))
                            .h_full()
                            .overflow_y_scroll()
                            .border_r_1()
                            .border_color(border)
                            .child(section_header("Versions"))
                            .when(rows_empty, |this| {
                                this.child(empty_state(
                                    "No releases yet",
                                    format!("Publish a release tagged {app}-v<version>."),
                                ))
                            })
                            .children(version_rows),
                    )
                    .child(
                        v_flex()
                            .id("notes")
                            .flex_1()
                            .h_full()
                            .overflow_y_scroll()
                            .child(section_header("Release notes"))
                            .child(self.render_notes(notes_release, installed_notes)),
                    ),
            )
    }

    fn render_header(&self, app: AppId, cx: &mut Context<Self>) -> AnyElement {
        let info = app.info();
        let entry = self.state.catalog_entry(app).cloned();
        let summary = entry
            .as_ref()
            .map(|entry| entry.summary.clone())
            .unwrap_or_else(|| info.description.to_string());
        let highlights = entry
            .as_ref()
            .map(|entry| entry.highlights.clone())
            .unwrap_or_default();
        let roadmap = entry
            .as_ref()
            .map(|entry| entry.roadmap.clone())
            .unwrap_or_default();
        let homepage = entry.as_ref().and_then(|entry| entry.homepage.clone());
        let channel = self.state.channel(app);

        let bullets = |items: Vec<String>, prefix: &'static str| {
            v_flex()
                .gap_0p5()
                .children(items.into_iter().map(move |item| {
                    Label::new(format!("{prefix} {item}"))
                        .size(LabelSize::Small)
                        .color(Color::Muted)
                }))
        };
        let channel_buttons: Vec<AnyElement> = UpdateChannel::ALL
            .into_iter()
            .map(|option| {
                Button::new(
                    SharedString::from(format!("channel-{}", option.label())),
                    option.label(),
                )
                .style(ButtonStyle::Subtle)
                .toggle_state(channel == option)
                .on_click(cx.listener(move |this, _, _, cx| this.set_channel(app, option, cx)))
                .into_any_element()
            })
            .collect();

        h_flex()
            .gap_4()
            .items_start()
            .child(logo(app, px(64.)))
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .child(
                        h_flex()
                            .gap_2()
                            .child(Headline::new(info.name).size(HeadlineSize::Small))
                            .when_some(entry.as_ref().map(|entry| entry.status), |this, status| {
                                this.child(status_badge(status, cx))
                            }),
                    )
                    .child(Label::new(summary).color(Color::Muted))
                    .when(!highlights.is_empty(), |this| {
                        this.child(bullets(highlights, "•"))
                    })
                    .when(!roadmap.is_empty(), |this| {
                        this.child(
                            Label::new("Next")
                                .size(LabelSize::XSmall)
                                .color(Color::Placeholder),
                        )
                        .child(bullets(roadmap, "→"))
                    }),
            )
            .child(
                v_flex()
                    .flex_none()
                    .items_end()
                    .gap_1()
                    .child(
                        Label::new("Update channel")
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    )
                    .child(h_flex().gap_1().children(channel_buttons))
                    .when_some(homepage, |this, url| {
                        this.child(
                            Button::new("homepage", "Website")
                                .style(ButtonStyle::Subtle)
                                .start_icon(Icon::new(IconName::ArrowUpRight))
                                .on_click(move |_, _, cx| cx.open_url(&url)),
                        )
                    }),
            )
            .into_any_element()
    }

    fn render_actions(
        &self,
        app: AppId,
        active: Option<Version>,
        latest: Option<ReleaseInfo>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let downloading = self.downloads.contains_key(&app);
        let installed_app = self
            .services
            .paths
            .application_path(app)
            .is_some_and(|path| path.is_dir());
        let update = self.state.update_for(app).map(|r| r.version.clone());
        let install = latest
            .as_ref()
            .filter(|_| active.is_none())
            .map(|r| r.version.clone());

        h_flex()
            .gap_2()
            .when_some(active.clone(), |this, version| {
                let running = self.is_running(app, &version);
                this.when(app != AppId::Toolbox, |this| {
                    this.child(
                        Button::new("launch", "Launch")
                            .style(ButtonStyle::Filled)
                            .start_icon(Icon::new(IconName::PlayFilled))
                            .on_click(cx.listener(move |this, _, _, cx| this.launch(app, cx))),
                    )
                })
                .child(
                    Label::new(if running {
                        format!("✓ Installed and running: v{version}")
                    } else {
                        format!("Active: v{version}")
                    })
                    .size(LabelSize::Small)
                    .color(if running {
                        Color::Success
                    } else {
                        Color::Muted
                    }),
                )
                .when(installed_app, |this| {
                    this.child(
                        Label::new("· in ~/Applications/Ferrum (Launchpad, Spotlight)")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                })
            })
            .when_some(update, |this, version| {
                this.child(
                    Button::new("update", format!("Update to v{version}"))
                        .start_icon(Icon::new(IconName::Download))
                        .disabled(downloading)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install(app, version.clone(), true, cx)
                        })),
                )
            })
            .when_some(install, |this, version| {
                this.child(
                    Button::new("install", format!("Install v{version}"))
                        .style(ButtonStyle::Filled)
                        .start_icon(Icon::new(IconName::Download))
                        .disabled(downloading)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.install(app, version.clone(), true, cx)
                        })),
                )
            })
            .when(active.is_none() && latest.is_none(), |this| {
                this.child(
                    Label::new("No published release yet. Check for updates or review Settings.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .when_some(
                active.filter(|_| self.state.app(app).installed.len() == 1),
                |this, version| {
                    this.child(
                        Button::new("uninstall", "Uninstall")
                            .style(ButtonStyle::Subtle)
                            .start_icon(Icon::new(IconName::Trash))
                            .disabled(downloading)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.uninstall(app, version.clone(), cx)
                            })),
                    )
                },
            )
            .into_any_element()
    }

    fn render_download(&self, app: AppId, cx: &mut Context<Self>) -> AnyElement {
        let Some(download) = self.downloads.get(&app) else {
            return div().into_any_element();
        };
        let fraction = download.fraction();
        let label = match fraction {
            Some(f) => format!("Downloading v{} — {:.0}%", download.version, f * 100.0),
            None => format!("Downloading v{}…", download.version),
        };
        h_flex()
            .gap_3()
            .child(Label::new(label).size(LabelSize::Small))
            .child(div().flex_1().child(ferrum_ui::ui::ProgressBar::new(
                "download-progress",
                fraction.unwrap_or(0.0),
                1.0,
                cx,
            )))
            .child(
                Button::new("cancel-download", "Cancel")
                    .style(ButtonStyle::Subtle)
                    .on_click(cx.listener(move |this, _, _, cx| this.cancel_download(app, cx))),
            )
            .into_any_element()
    }

    fn render_version_row(
        &self,
        app: AppId,
        row: VersionRow,
        downloading: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let version = row.version.clone();
        let selected = self.selected_release.as_ref() == Some(&version);
        let date = row
            .release
            .as_ref()
            .and_then(|r| r.published_at)
            .map(|t| {
                t.with_timezone(&chrono::Local)
                    .format("%Y-%m-%d")
                    .to_string()
            })
            .unwrap_or_else(|| "—".into());
        let target_asset = current_target().and_then(|target| {
            row.release
                .as_ref()
                .and_then(|r| r.assets_for(target).map(|(asset, _)| asset.size))
        });
        let size = match (&row.release, target_asset) {
            (_, Some(size)) => format_size(size),
            (Some(_), None) => "No build for this platform".into(),
            (None, None) => "Release removed".into(),
        };
        let channel = row.release.as_ref().map(ReleaseInfo::channel);
        let downloads = row.release.as_ref().map(ReleaseInfo::downloads);
        let can_install = !row.installed && !row.running && target_asset.is_some() && !downloading;

        ListItem::new(SharedString::from(format!("version-{version}")))
            .toggle_state(selected)
            .child(
                h_flex()
                    .w_full()
                    .gap_2()
                    .child(Label::new(format!("v{version}")))
                    .when_some(channel, |this, channel| {
                        this.child(channel_badge(channel, cx))
                    })
                    .when(row.active, |this| {
                        this.child(badge("active", cx.theme().status().success, cx))
                    })
                    .when(row.running, |this| {
                        this.child(badge("running", cx.theme().status().info, cx))
                    })
                    .when(row.installed && !row.active, |this| {
                        this.child(badge("installed", cx.theme().colors().text_muted, cx))
                    })
                    .child(div().flex_1())
                    .child(
                        Label::new(match downloads {
                            Some(downloads) => {
                                format!("{date} · {size} · {} downloads", format_count(downloads))
                            }
                            None => format!("{date} · {size}"),
                        })
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                    ),
            )
            .end_slot(
                h_flex()
                    .gap_1()
                    .when(can_install, |this| {
                        let version = version.clone();
                        this.child(
                            Button::new(
                                SharedString::from(format!("install-{version}")),
                                "Install",
                            )
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(
                                move |this, _, _, cx| this.install(app, version.clone(), false, cx),
                            )),
                        )
                    })
                    .when(row.installed && !row.active && !downloading, |this| {
                        let activate_version = version.clone();
                        let remove_version = version.clone();
                        this.child(
                            Button::new(
                                SharedString::from(format!("activate-{version}")),
                                "Activate",
                            )
                            .style(ButtonStyle::Subtle)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.activate(app, activate_version.clone(), cx)
                                },
                            )),
                        )
                        .child(
                            IconButton::new(
                                SharedString::from(format!("remove-{version}")),
                                IconName::Trash,
                            )
                            .tooltip(Tooltip::text("Remove this version"))
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.uninstall(app, remove_version.clone(), cx)
                                },
                            )),
                        )
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| {
                this.selected_release = Some(version.clone());
                cx.notify();
            }))
            .into_any_element()
    }

    fn render_notes(
        &self,
        release: Option<ReleaseInfo>,
        installed_notes: Option<(Version, String)>,
    ) -> impl IntoElement {
        let (title, notes, link) = match (release, installed_notes) {
            (Some(release), _) => (
                format!("{} — {}", release.tag, release.title),
                release.notes,
                Some(release.html_url).filter(|url| !url.is_empty()),
            ),
            (None, Some((version, notes))) => (format!("v{version} (installed)"), notes, None),
            (None, None) => (String::new(), String::new(), None),
        };
        let notes = if notes.trim().is_empty() && !title.is_empty() {
            "No release notes.".to_string()
        } else {
            notes
        };
        v_flex()
            .gap_2()
            .px_3()
            .pb_4()
            .when(!title.is_empty(), |this| {
                this.child(Label::new(title).size(LabelSize::Small).color(Color::Muted))
            })
            .child(div().w_full().text_sm().child(notes))
            .when_some(link, |this, url| {
                this.child(
                    Button::new("open-release", "Open on GitHub")
                        .style(ButtonStyle::Subtle)
                        .start_icon(Icon::new(IconName::ArrowUpRight))
                        .on_click(move |_, _, cx| cx.open_url(&url)),
                )
            })
    }
}

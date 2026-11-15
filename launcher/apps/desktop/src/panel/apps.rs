//! The Apps tab: installed apps (open, update, the ⋮ menu) then the ones
//! available to install, with download progress under a row.

use ferrum_ui::components::{ButtonSize, ProgressBar, TintColor};
use ferrum_ui::gpui::{Anchor, FontWeight};
use ferrum_ui::prelude::*;
use ferrumlauncher_core::{AppId, current_target};
use semver::Version;

use super::LauncherPanel;
use super::layout::{RowState, matches};
use crate::app::FerrumLauncher;
use crate::components::uninstall::can_uninstall;

/// One app row, read from the launcher before rendering.
struct AppRow {
    app: AppId,
    name: String,
    state: RowState,
    logo: AnyElement,
    /// Installed and not the launcher: the menu offers Uninstall….
    removable: bool,
}

fn row_state(launcher: &FerrumLauncher, app: AppId) -> RowState {
    let state = &launcher.state;
    let installable = state
        .latest(app)
        .filter(|release| current_target().is_some_and(|t| release.assets_for(t).is_some()))
        .map(|release| &release.version);
    RowState::new(
        state.active(app),
        state.update_for(app).map(|release| &release.version),
        installable,
        launcher
            .downloads
            .get(&app)
            .map(|download| (&download.version, download.fraction())),
    )
}

fn section(title: &'static str) -> AnyElement {
    div()
        .px_2()
        .pt_2()
        .pb_1()
        .child(
            Label::new(title)
                .size(LabelSize::XSmall)
                .weight(FontWeight::MEDIUM)
                .color(Color::Muted),
        )
        .into_any_element()
}

impl LauncherPanel {
    fn app_rows(&self, cx: &App) -> Vec<AppRow> {
        let query = self.query(cx);
        let launcher = self.launcher.read(cx);
        launcher
            .state
            .managed_apps()
            .into_iter()
            .chain(launcher.state.discontinued_apps())
            .filter(|info| matches(&query, &info.name))
            .map(|info| {
                let app = info.id;
                AppRow {
                    app,
                    name: info.name,
                    state: row_state(launcher, app),
                    logo: launcher.logo(app, px(32.)).into_any_element(),
                    removable: can_uninstall(launcher, app),
                }
            })
            .collect()
    }

    pub(super) fn render_apps(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let (installed, available): (Vec<_>, Vec<_>) = self
            .app_rows(cx)
            .into_iter()
            .partition(|row| row.state.installed());
        let mut items = Vec::new();
        if installed.is_empty() && available.is_empty() {
            let query = self.query(cx);
            items.push(
                div()
                    .p_4()
                    .child(
                        Label::new(format!("No app matches “{query}”"))
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .into_any_element(),
            );
        }
        if !installed.is_empty() {
            items.push(section("Installed"));
            items.extend(
                installed
                    .into_iter()
                    .map(|row| self.render_app_row(row, cx)),
            );
        }
        if !available.is_empty() {
            items.push(section("Available"));
            items.extend(
                available
                    .into_iter()
                    .map(|row| self.render_app_row(row, cx)),
            );
        }
        items
    }

    fn render_app_row(&self, row: AppRow, cx: &mut Context<Self>) -> AnyElement {
        let app = row.app;
        let installed = row.state.installed();
        let hover = ferrum_ui::hover_fill(cx);
        let action = match &row.state {
            RowState::Update { to, .. } => {
                let to = to.clone();
                Some(
                    Button::new(SharedString::from(format!("panel-update-{app}")), "Update")
                        .size(ButtonSize::Compact)
                        .style(ButtonStyle::Tinted(TintColor::Accent))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            let to = to.clone();
                            this.launcher
                                .update(cx, |l, cx| l.install(app, to, true, cx));
                        })),
                )
            }
            RowState::Install(version) => {
                let version = version.clone();
                Some(
                    Button::new(
                        SharedString::from(format!("panel-install-{app}")),
                        "Install",
                    )
                    .size(ButtonSize::Compact)
                    .style(ButtonStyle::Filled)
                    .on_click(cx.listener(move |this, _, _, cx| {
                        let version = version.clone();
                        this.launcher
                            .update(cx, |l, cx| l.install(app, version, true, cx));
                    })),
                )
            }
            _ => None,
        };
        h_flex()
            .id(SharedString::from(format!("panel-app-{app}")))
            .items_start()
            .px_2()
            .py_1p5()
            .gap_2p5()
            .rounded(px(8.))
            .when(installed, |this| {
                this.cursor_pointer()
                    .hover(move |style| style.bg(hover))
                    .tooltip(Tooltip::text(format!("Open {}", row.name)))
                    .on_click(cx.listener(move |this, _, window, cx| this.launch(app, window, cx)))
            })
            .child(row.logo)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .child(
                        Label::new(row.name.clone())
                            .weight(FontWeight::SEMIBOLD)
                            .truncate(),
                    )
                    .child(render_versions(&row.state))
                    .children(self.render_progress(app, &row.state, cx)),
            )
            .children(action)
            .when(installed, |this| {
                this.child(self.render_app_menu(app, row.removable, cx))
            })
            .into_any_element()
    }

    fn render_progress(
        &self,
        app: AppId,
        state: &RowState,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let RowState::Downloading { fraction, .. } = state else {
            return None;
        };
        let text = match fraction {
            Some(fraction) => format!("Downloading {:.0}%", fraction * 100.),
            None => "Downloading…".to_string(),
        };
        Some(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .child(
                            div().flex_1().child(
                                Label::new(text)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Accent),
                            ),
                        )
                        .child(
                            Button::new(
                                SharedString::from(format!("panel-cancel-{app}")),
                                "Cancel",
                            )
                            .size(ButtonSize::Compact)
                            .label_size(LabelSize::XSmall)
                            .on_click(cx.listener(
                                move |this, _, _, cx| {
                                    this.launcher.update(cx, |l, cx| l.cancel_download(app, cx));
                                },
                            )),
                        ),
                )
                .child(ProgressBar::new(
                    SharedString::from(format!("panel-progress-{app}")),
                    fraction.unwrap_or(0.),
                    1.,
                    cx,
                ))
                .into_any_element(),
        )
    }

    fn render_app_menu(
        &self,
        app: AppId,
        removable: bool,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let weak = cx.weak_entity();
        PopoverMenu::new(SharedString::from(format!("panel-app-menu-{app}")))
            .anchor(Anchor::TopRight)
            .trigger(
                IconButton::new(
                    SharedString::from(format!("panel-app-more-{app}")),
                    IconName::Ellipsis,
                )
                .icon_size(IconSize::Small)
                .icon_color(Color::Muted),
            )
            .menu(move |window, cx| {
                let weak = weak.clone();
                Some(ContextMenu::build(window, cx, move |menu, _, _| {
                    let [open, reveal, versions, remove, settings] =
                        std::array::from_fn(|_| weak.clone());
                    let menu = menu
                        .icon_entry("Open", IconName::PlayFilled, move |window, cx| {
                            open.update(cx, |this, cx| this.launch(app, window, cx))
                                .ok();
                        })
                        .icon_entry("Show in Finder", IconName::Folder, move |_, cx| {
                            reveal.update(cx, |this, cx| this.reveal(app, cx)).ok();
                        })
                        .icon_entry("Versions…", IconName::Clock, move |window, cx| {
                            versions
                                .update(cx, |this, cx| this.show_main(Some(app), false, window, cx))
                                .ok();
                        });
                    // Asks in the main window, which lists what is removed.
                    let menu = if removable {
                        menu.icon_entry("Uninstall…", IconName::Trash, move |window, cx| {
                            remove
                                .update(cx, |this, cx| {
                                    this.show_main(Some(app), false, window, cx);
                                    this.launcher
                                        .update(cx, |l, cx| l.confirm_uninstall(app, cx));
                                })
                                .ok();
                        })
                    } else {
                        menu
                    };
                    menu.separator().icon_entry(
                        "Settings…",
                        IconName::Settings,
                        move |window, cx| {
                            settings
                                .update(cx, |this, cx| this.show_main(Some(app), false, window, cx))
                                .ok();
                        },
                    )
                }))
            })
    }

    fn launch(&mut self, app: AppId, window: &mut Window, cx: &mut Context<Self>) {
        self.launcher.update(cx, |l, cx| l.launch(app, cx));
        self.close(window, cx);
    }

    /// Shows the app in Finder: its copy in ~/Applications/Ferrum, else the
    /// installed bundle.
    fn reveal(&mut self, app: AppId, cx: &mut Context<Self>) {
        let launcher = self.launcher.read(cx);
        let paths = &launcher.services.paths;
        let info = launcher.state.info(app);
        let path = paths
            .application_path(&info)
            .filter(|path| path.exists())
            .or_else(|| {
                let version = launcher.state.active(app)?;
                paths
                    .bundle_path(app, version)
                    .or_else(|| Some(paths.version_dir(app, version)))
            });
        match path {
            Some(path) => cx.reveal_path(&path),
            None => self.launcher.update(cx, |l, cx| {
                l.notify_user(format!("{} isn't installed", info.name), true, cx)
            }),
        }
    }
}

/// `0.7.0` for an app that's current, `0.7.0 → 0.7.1` (the new version in
/// the accent colour) when one is out.
fn render_versions(state: &RowState) -> AnyElement {
    let muted = |text: String| Label::new(text).size(LabelSize::XSmall).color(Color::Muted);
    let arrow = |from: &Version, to: &Version| {
        h_flex()
            .gap_1()
            .child(muted(from.to_string()))
            .child(muted("→".into()))
            .child(
                Label::new(to.to_string())
                    .size(LabelSize::XSmall)
                    .color(Color::Accent),
            )
            .into_any_element()
    };
    match state {
        RowState::UpToDate(version) => muted(version.to_string()).into_any_element(),
        RowState::Update { from, to }
        | RowState::Downloading {
            from: Some(from),
            to,
            ..
        } => arrow(from, to),
        RowState::Downloading { from: None, to, .. } => {
            muted(format!("Installing {to}")).into_any_element()
        }
        RowState::Install(version) => muted(version.to_string()).into_any_element(),
        RowState::Unavailable => muted("No build for this computer yet".into()).into_any_element(),
    }
}

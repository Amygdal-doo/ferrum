//! Overview: every app with its status, versions and the next action.
//! Clicking a row opens the app's page.

use super::uninstall::can_uninstall;
use super::{discontinued_badge, format_count, format_date, status_badge};
use crate::app::FerrumLauncher;
use ferrum_ui::components::right_click_menu;
use ferrum_ui::prelude::*;
use ferrumlauncher_core::catalog::AppGroup;
use ferrumlauncher_core::{AppId, Channel, current_target};

const STATUS_WIDTH: f32 = 116.;
const VERSION_WIDTH: f32 = 96.;
const DATE_WIDTH: f32 = 92.;
const DOWNLOADS_WIDTH: f32 = 82.;
const ACTION_WIDTH: f32 = 150.;
const APP_MIN_WIDTH: f32 = 220.;
/// Below this the table scrolls sideways instead of squeezing the App column.
const TABLE_MIN_WIDTH: f32 = APP_MIN_WIDTH
    + STATUS_WIDTH
    + 3. * VERSION_WIDTH
    + DATE_WIDTH
    + DOWNLOADS_WIDTH
    + ACTION_WIDTH
    + 7. * 12. // gap_3 between the 8 columns
    + 32.; // px_4 on both sides

fn cell(width: f32) -> Div {
    div().flex_none().w(px(width)).overflow_hidden()
}

/// A section heading inside the table ("Apps", "Tools", …).
fn group_header(title: &'static str, cx: &App) -> AnyElement {
    h_flex()
        .px_4()
        .pt_3()
        .pb_1()
        .border_b_1()
        .border_color(cx.theme().colors().border_variant)
        .child(
            Label::new(title)
                .size(LabelSize::Small)
                .weight(ferrum_ui::gpui::FontWeight::SEMIBOLD)
                .color(Color::Muted),
        )
        .into_any_element()
}

fn version_label(version: Option<String>) -> Label {
    match version {
        Some(version) => Label::new(format!("v{version}")).size(LabelSize::Small),
        None => Label::new("—")
            .size(LabelSize::Small)
            .color(Color::Placeholder),
    }
}

impl FerrumLauncher {
    pub fn render_overview(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        // The Apps, then the Tools, then the launcher itself, then installed
        // apps the suite dropped.
        let managed = self.state.managed_apps();
        let mut rows: Vec<AnyElement> = Vec::new();
        for group in AppGroup::ALL {
            let ids: Vec<AppId> = managed
                .iter()
                .filter(|info| info.group == group && info.id != AppId::LAUNCHER)
                .map(|info| info.id)
                .collect();
            if ids.is_empty() {
                continue;
            }
            rows.push(group_header(group.label(), cx));
            rows.extend(ids.into_iter().map(|app| self.render_overview_row(app, cx)));
        }
        if managed.iter().any(|info| info.id == AppId::LAUNCHER) {
            rows.push(group_header("Launcher", cx));
            rows.push(self.render_overview_row(AppId::LAUNCHER, cx));
        }
        let discontinued = self.state.discontinued_apps();
        if !discontinued.is_empty() {
            rows.push(group_header("No longer offered", cx));
            rows.extend(
                discontinued
                    .into_iter()
                    .map(|info| self.render_overview_row(info.id, cx)),
            );
        }
        let colors = cx.theme().colors();

        let header = |text: &'static str, width: Option<f32>| {
            let label = Label::new(text).size(LabelSize::XSmall).color(Color::Muted);
            match width {
                Some(width) => cell(width).child(label),
                None => div().flex_1().min_w(px(APP_MIN_WIDTH)).child(label),
            }
        };

        v_flex()
            .id("overview")
            .size_full()
            .overflow_y_scroll()
            .child(
                div()
                    .id("overview-table")
                    .w_full()
                    .overflow_x_scroll()
                    .child(
                        v_flex()
                            .w_full()
                            .min_w(px(TABLE_MIN_WIDTH))
                            .child(
                                h_flex()
                                    .px_4()
                                    .py_2()
                                    .gap_3()
                                    .border_b_1()
                                    .border_color(colors.border_variant)
                                    .child(header("App", None))
                                    .child(header("Status", Some(STATUS_WIDTH)))
                                    .child(header("Installed", Some(VERSION_WIDTH)))
                                    .child(header("Latest stable", Some(VERSION_WIDTH)))
                                    .child(header("", Some(ACTION_WIDTH)))
                                    .child(header("Latest alpha", Some(VERSION_WIDTH)))
                                    .child(header("Released", Some(DATE_WIDTH)))
                                    .child(header("Downloads", Some(DOWNLOADS_WIDTH))),
                            )
                            .children(rows),
                    ),
            )
    }

    fn render_overview_row(&self, app: AppId, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors();
        let info = self.state.info(app);
        let is_new = self.state.is_new(app);
        let offered = self.state.is_offered(app);
        let entry = self.state.catalog_entry(app);
        let status = entry.map(|entry| entry.status);
        let summary = entry
            .map(|entry| entry.summary.clone())
            .unwrap_or_else(|| info.description.clone());
        let active = self.state.active(app).map(ToString::to_string);
        let stable = self
            .state
            .latest_in(app, Channel::Stable)
            .map(|r| r.version.to_string());
        let preview = self
            .state
            .latest_in(app, Channel::Alpha)
            .into_iter()
            .chain(self.state.latest_in(app, Channel::Beta))
            .max_by(|a, b| a.version.cmp(&b.version))
            .map(|r| r.version.to_string());
        let released = format_date(self.state.last_released(app));
        let downloads = self.state.total_downloads(app);
        let has_releases = !self.state.releases_for(app).is_empty();

        let row = h_flex()
            .id(SharedString::from(format!("overview-{app}")))
            .px_4()
            .py_2()
            .gap_3()
            .border_b_1()
            .border_color(colors.border_variant)
            .cursor_pointer()
            .hover(|style| style.bg(colors.ghost_element_hover))
            .child(
                h_flex()
                    .flex_1()
                    .min_w(px(APP_MIN_WIDTH))
                    .gap_3()
                    .child(self.logo(app, px(36.)))
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .child(Label::new(info.name).truncate())
                                    .when(is_new, |this| {
                                        this.child(badge("New", cx.theme().status().info, cx))
                                    }),
                            )
                            .child(
                                Label::new(summary)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .truncate(),
                            ),
                    ),
            )
            .child(
                cell(STATUS_WIDTH)
                    .when(!offered, |this| this.child(discontinued_badge(cx)))
                    .when_some(status.filter(|_| offered), |this, status| {
                        this.child(status_badge(status, cx))
                    }),
            )
            .child(cell(VERSION_WIDTH).child(version_label(active)))
            .child(cell(VERSION_WIDTH).child(version_label(stable)))
            .child(cell(ACTION_WIDTH).child(self.render_overview_action(app, cx)))
            .child(cell(VERSION_WIDTH).child(version_label(preview)))
            .child(
                cell(DATE_WIDTH).child(
                    Label::new(released)
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            )
            .child(
                cell(DOWNLOADS_WIDTH).child(
                    Label::new(if has_releases {
                        format_count(downloads)
                    } else {
                        "—".into()
                    })
                    .size(LabelSize::Small)
                    .color(Color::Muted),
                ),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_app(app, cx)));
        self.with_row_menu(app, row, cx)
    }

    /// Right-click menu of an overview row: Open, Details and Uninstall….
    fn with_row_menu(
        &self,
        app: AppId,
        row: impl IntoElement + 'static,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let weak = cx.weak_entity();
        let installed = self.state.active(app).is_some();
        let removable = can_uninstall(self, app);
        right_click_menu(SharedString::from(format!("overview-menu-{app}")))
            .trigger(move |_, _, _| row)
            .menu(move |window, cx| {
                let weak = weak.clone();
                ContextMenu::build(window, cx, move |menu, _, _| {
                    let [open, details, uninstall] = std::array::from_fn(|_| weak.clone());
                    let menu = if installed && app != AppId::LAUNCHER {
                        menu.icon_entry("Open", IconName::PlayFilled, move |_, cx| {
                            open.update(cx, |this, cx| this.launch(app, cx)).ok();
                        })
                    } else {
                        menu
                    };
                    let menu = menu.icon_entry("Details", IconName::Info, move |_, cx| {
                        details.update(cx, |this, cx| this.select_app(app, cx)).ok();
                    });
                    if removable {
                        menu.separator().icon_entry(
                            "Uninstall…",
                            IconName::Trash,
                            move |_, cx| {
                                uninstall
                                    .update(cx, |this, cx| this.confirm_uninstall(app, cx))
                                    .ok();
                            },
                        )
                    } else {
                        menu
                    }
                })
            })
            .into_any_element()
    }

    fn render_overview_action(&self, app: AppId, cx: &mut Context<Self>) -> AnyElement {
        if let Some(download) = self.downloads.get(&app) {
            let text = match download.fraction() {
                Some(fraction) => format!("Downloading {:.0}%", fraction * 100.0),
                None => "Downloading…".into(),
            };
            return Label::new(text)
                .size(LabelSize::Small)
                .color(Color::Accent)
                .into_any_element();
        }
        let active = self.state.active(app).cloned();
        if let Some(restart) = self.restart_button("overview-restart", app, cx) {
            return restart;
        }
        if let Some(update) = self.state.update_for(app) {
            let version = update.version.clone();
            return Button::new(
                SharedString::from(format!("overview-update-{app}")),
                format!("Update to v{version}"),
            )
            .style(ButtonStyle::Filled)
            .on_click(
                cx.listener(move |this, _, _, cx| this.install(app, version.clone(), true, cx)),
            )
            .into_any_element();
        }
        if active.is_some() {
            if app == AppId::LAUNCHER {
                return Label::new("✓ Up to date")
                    .size(LabelSize::Small)
                    .color(Color::Success)
                    .into_any_element();
            }
            return h_flex()
                .gap_2()
                .child(
                    Button::new(SharedString::from(format!("overview-open-{app}")), "Open")
                        .start_icon(Icon::new(IconName::PlayFilled))
                        .on_click(cx.listener(move |this, _, _, cx| this.launch(app, cx))),
                )
                .child(Label::new("✓").color(Color::Success))
                .into_any_element();
        }
        match self.state.latest(app) {
            Some(latest)
                if current_target().is_some_and(|target| latest.assets_for(target).is_some()) =>
            {
                let version = latest.version.clone();
                Button::new(
                    SharedString::from(format!("overview-install-{app}")),
                    format!("Install v{version}"),
                )
                .style(ButtonStyle::Filled)
                .start_icon(Icon::new(IconName::Download))
                .on_click(
                    cx.listener(move |this, _, _, cx| this.install(app, version.clone(), true, cx)),
                )
                .into_any_element()
            }
            Some(_) => Label::new("No build for this OS")
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .into_any_element(),
            None => Label::new("—")
                .size(LabelSize::Small)
                .color(Color::Placeholder)
                .into_any_element(),
        }
    }
}

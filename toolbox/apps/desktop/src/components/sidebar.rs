//! Navigation: Overview, Releases and the app list.

use super::logo::logo;
use super::status_badge;
use crate::app::{FerrumToolbox, Page};
use ferrum_ui::prelude::*;
use ferrumtoolbox_core::AppId;

impl FerrumToolbox {
    pub fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().colors().panel_background;
        let rows: Vec<AnyElement> = AppId::MANAGED
            .into_iter()
            .map(|app| self.render_app_row(app, cx))
            .collect();
        let updates = self.state.app_updates().len();
        v_flex()
            .id("app-list")
            .size_full()
            .overflow_y_scroll()
            .bg(background)
            .py_1()
            .child(
                self.render_nav_item(
                    "nav-overview",
                    IconName::Blocks,
                    "Overview",
                    Page::Overview,
                    (updates > 0).then(|| {
                        format!("{updates} update{}", if updates == 1 { "" } else { "s" })
                    }),
                    cx,
                ),
            )
            .child(self.render_nav_item(
                "nav-releases",
                IconName::HistoryRerun,
                "Releases",
                Page::Releases,
                Some(self.state.releases.len().to_string()).filter(|count| count != "0"),
                cx,
            ))
            .child(div().h_2())
            .child(section_header("Apps"))
            .children(rows)
    }

    fn render_nav_item(
        &self,
        id: &'static str,
        icon: IconName,
        label: &'static str,
        page: Page,
        trailing: Option<String>,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        ListItem::new(id)
            .toggle_state(self.page == page)
            .start_slot(Icon::new(icon).color(Color::Muted))
            .child(Label::new(label))
            .when_some(trailing, |this, text| {
                this.end_slot(Label::new(text).size(LabelSize::XSmall).color(Color::Muted))
            })
            .on_click(cx.listener(move |this, _, _, cx| this.show_page(page, cx)))
            .into_any_element()
    }

    fn render_app_row(&self, app: AppId, cx: &mut Context<Self>) -> AnyElement {
        let info = app.info();
        let active = self.state.active(app).cloned();
        let latest = self.state.latest(app).map(|r| r.version.clone());
        let update = self.state.update_for(app).map(|r| r.version.clone());
        let line = match (&active, &update, &latest) {
            (Some(_), Some(update), _) => format!("Update to v{update}"),
            (Some(active), None, _) => format!("v{active} · active"),
            (None, _, Some(latest)) => format!("v{latest} available"),
            (None, _, None) if self.state.releases_for(app).is_empty() => {
                "No releases yet".to_string()
            }
            (None, _, None) => "Only prereleases".to_string(),
        };
        let line_color = if update.is_some() || (active.is_none() && latest.is_some()) {
            Color::Accent
        } else {
            Color::Muted
        };
        let status = self.state.catalog_entry(app).map(|entry| entry.status);
        let downloading = self.downloads.get(&app).map(|d| d.fraction());

        ListItem::new(SharedString::from(format!("app-{app}")))
            .toggle_state(self.page == Page::App && self.selected == app)
            .start_slot(logo(app, px(28.)))
            .child(
                v_flex()
                    .py_0p5()
                    .child(
                        h_flex()
                            .gap_1p5()
                            .child(Label::new(info.name))
                            .when_some(status, |this, status| this.child(status_badge(status, cx))),
                    )
                    .child(Label::new(line).size(LabelSize::XSmall).color(line_color)),
            )
            .when_some(downloading, |this, fraction| {
                let text = fraction
                    .map(|f| format!("{:.0}%", f * 100.0))
                    .unwrap_or_else(|| "…".into());
                this.end_slot(
                    Label::new(text)
                        .size(LabelSize::XSmall)
                        .color(Color::Accent),
                )
            })
            .on_click(cx.listener(move |this, _, _, cx| this.select_app(app, cx)))
            .into_any_element()
    }
}

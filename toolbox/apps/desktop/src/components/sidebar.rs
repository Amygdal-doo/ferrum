//! App list.

use super::icon_for;
use crate::app::FerrumToolbox;
use ferrum_ui::prelude::*;
use ferrumtoolbox_core::AppId;

impl FerrumToolbox {
    pub fn render_sidebar(&mut self, cx: &mut Context<Self>) -> impl IntoElement {
        let background = cx.theme().colors().panel_background;
        let rows: Vec<AnyElement> = AppId::ALL
            .into_iter()
            .map(|app| self.render_app_row(app, cx))
            .collect();
        v_flex()
            .id("app-list")
            .size_full()
            .overflow_y_scroll()
            .bg(background)
            .py_1()
            .child(section_header("Apps"))
            .children(rows)
    }

    fn render_app_row(&self, app: AppId, cx: &mut Context<Self>) -> AnyElement {
        let info = app.info();
        let status = match self.state.active(app) {
            Some(version) => format!("v{version}"),
            None => "Not installed".to_string(),
        };
        let update = self.state.update_for(app).map(|r| r.version.clone());
        let downloading = self.downloads.get(&app).map(|d| d.fraction());

        ListItem::new(SharedString::from(format!("app-{app}")))
            .toggle_state(self.selected == app)
            .start_slot(Icon::new(icon_for(app)).color(Color::Muted))
            .child(
                v_flex().py_0p5().child(Label::new(info.name)).child(
                    Label::new(status)
                        .size(LabelSize::XSmall)
                        .color(Color::Muted),
                ),
            )
            .end_slot(
                h_flex()
                    .gap_1()
                    .when_some(downloading, |this, fraction| {
                        let text = fraction
                            .map(|f| format!("{:.0}%", f * 100.0))
                            .unwrap_or_else(|| "…".into());
                        this.child(
                            Label::new(text)
                                .size(LabelSize::XSmall)
                                .color(Color::Accent),
                        )
                    })
                    .when_some(update.filter(|_| downloading.is_none()), |this, version| {
                        this.child(badge(
                            format!("Update v{version}"),
                            cx.theme().status().info,
                            cx,
                        ))
                    }),
            )
            .on_click(cx.listener(move |this, _, _, cx| this.select_app(app, cx)))
            .into_any_element()
    }
}

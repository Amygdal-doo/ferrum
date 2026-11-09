//! Lazy table tree under each connection.

use crate::app::FerrumData;
use crate::state::SchemaLoad;
use ferrum_ui::prelude::*;
use ferrumdata_domain::{DatabaseEngine, ProfileId, TableInfo, TableKind};

/// Max rows rendered in one tree; beyond this the user is asked to filter.
/// Keeps element count bounded for schemas with thousands of objects.
const MAX_RENDERED: usize = 500;

impl FerrumData {
    pub fn render_table_tree(
        &mut self,
        profile_id: ProfileId,
        filter: &str,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let muted = |text: SharedString| {
            div()
                .py_1()
                .pl(px(28.))
                .text_xs()
                .text_color(colors.text_muted)
                .child(text)
        };

        let tables = match self.state.schemas.get(&profile_id) {
            None | Some(SchemaLoad::Loading) => {
                return muted("Loading tables…".into()).into_any_element();
            }
            Some(SchemaLoad::Failed(e)) => {
                return h_flex()
                    .gap_2()
                    .py_1()
                    .pl(px(28.))
                    .text_xs()
                    .text_color(cx.theme().status().error)
                    .child(e.clone())
                    .child(
                        Button::new(SharedString::from(format!("retry-{profile_id}")), "Retry")
                            .label_size(LabelSize::XSmall)
                            .on_click(
                                cx.listener(move |this, _, _, cx| this.load_tables(profile_id, cx)),
                            ),
                    )
                    .into_any_element();
            }
            Some(SchemaLoad::Loaded(t)) => t,
        };

        let needle = filter.to_lowercase();
        let matching: Vec<&TableInfo> = tables
            .iter()
            .filter(|t| needle.is_empty() || t.table.name.to_lowercase().contains(&needle))
            .collect();
        let hidden = matching.len().saturating_sub(MAX_RENDERED);
        let single_schema = tables
            .windows(2)
            .all(|w| w[0].table.schema == w[1].table.schema);
        let total = tables.len();
        let engine = self
            .state
            .profile(profile_id)
            .map_or(DatabaseEngine::Postgres, |p| p.engine);
        let active_table = self
            .state
            .active_tab()
            .filter(|t| t.profile_id == Some(profile_id))
            .and_then(|t| t.table.clone());

        let mut items: Vec<AnyElement> = Vec::new();
        let mut last_schema: Option<&str> = None;
        for (index, info) in matching.iter().take(MAX_RENDERED).enumerate() {
            let schema_name = info.table.schema.as_deref().unwrap_or("");
            if !single_schema && last_schema != Some(schema_name) {
                items.push(
                    div()
                        .pt_1()
                        .pl(px(24.))
                        .text_xs()
                        .text_color(colors.text_muted)
                        .child(schema_name.to_uppercase())
                        .into_any_element(),
                );
                last_schema = Some(schema_name);
            }
            let active = active_table.as_ref() == Some(&info.table);
            let icon = match info.kind {
                TableKind::View | TableKind::MaterializedView => "◇",
                _ => "▦",
            };
            let rows = info
                .estimated_rows
                .map(|n| format!("~{n} rows"))
                .unwrap_or_default();
            let title = format!(
                "{} ({}) {rows}",
                info.table.qualified(engine),
                info.kind.label()
            );
            let open = (*info).clone();
            items.push(
                h_flex()
                    .id(SharedString::from(format!("table-{profile_id}-{index}")))
                    .gap_1p5()
                    .rounded_sm()
                    .py_0p5()
                    .pl(px(28.))
                    .pr_2()
                    .text_xs()
                    .when(active, |this| this.bg(colors.element_selected))
                    .when(!active, |this| {
                        this.text_color(colors.text_muted)
                            .hover(|s| s.bg(colors.element_hover).text_color(colors.text))
                    })
                    .tooltip(Tooltip::text(title))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_table(profile_id, open.clone(), cx)
                    }))
                    .child(div().text_color(colors.text_muted).child(icon))
                    .child(div().truncate().child(info.table.name.clone()))
                    .into_any_element(),
            );
        }

        v_flex()
            .pb_1()
            .child(
                h_flex()
                    .gap_1()
                    .py_0p5()
                    .pl(px(24.))
                    .pr_1()
                    .text_xs()
                    .text_color(colors.text_muted)
                    .child(format!("Tables ({total})"))
                    .child(div().flex_1())
                    .child(
                        IconButton::new(
                            SharedString::from(format!("reload-{profile_id}")),
                            IconName::RotateCw,
                        )
                        .icon_size(IconSize::XSmall)
                        .tooltip(Tooltip::text("Reload tables"))
                        .on_click(
                            cx.listener(move |this, _, _, cx| this.load_tables(profile_id, cx)),
                        ),
                    ),
            )
            .when(matching.is_empty(), |this| {
                this.child(muted("No tables match.".into()))
            })
            .children(items)
            .when(hidden > 0, |this| {
                this.child(muted(
                    format!("{hidden} more — type in the filter to narrow down").into(),
                ))
            })
            .into_any_element()
    }
}

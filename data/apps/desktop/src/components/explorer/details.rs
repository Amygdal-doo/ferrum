//! Columns and indexes under an expanded table node.

use crate::app::FerrumData;
use crate::state::DetailsLoad;
use ferrum_ui::prelude::*;
use ferrumdata_domain::{ProfileId, TableRef};

const INDENT: f32 = 46.;

impl FerrumData {
    pub(super) fn render_table_details(
        &self,
        key: &(ProfileId, TableRef),
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let line = |text: String| {
            div()
                .py_0p5()
                .pl(px(INDENT))
                .text_xs()
                .text_color(colors.text_muted)
                .child(text)
        };
        let details = match self.state.table_details.get(key) {
            None | Some(DetailsLoad::Loading) => {
                return line("Loading columns…".into()).into_any_element();
            }
            Some(DetailsLoad::Failed(e)) => {
                let (profile_id, table) = key.clone();
                return h_flex()
                    .gap_2()
                    .py_0p5()
                    .pl(px(INDENT))
                    .text_xs()
                    .text_color(cx.theme().status().error)
                    .child(div().truncate().child(e.clone()))
                    .child(
                        Button::new(
                            SharedString::from(format!(
                                "details-retry-{profile_id}-{}",
                                table.name
                            )),
                            "Retry",
                        )
                        .label_size(LabelSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.load_table_details(profile_id, table.clone(), cx)
                        })),
                    )
                    .into_any_element();
            }
            Some(DetailsLoad::Loaded(d)) => d,
        };

        // Name in the normal colour, the rest muted; the tooltip has it all.
        let entry = |id: String, name: &str, summary: String, key_marker: bool| {
            h_flex()
                .id(SharedString::from(id))
                .gap_1p5()
                .py_px()
                .pl(px(INDENT))
                .pr_2()
                .text_xs()
                .tooltip(Tooltip::text(format!("{name}  {summary}")))
                .child(
                    div()
                        .flex_none()
                        .w(px(10.))
                        .text_color(colors.text_muted)
                        .child(if key_marker { "⚷" } else { "·" }),
                )
                .child(div().flex_none().child(name.to_owned()))
                .child(
                    div()
                        .truncate()
                        .text_color(colors.text_muted)
                        .child(summary),
                )
                .into_any_element()
        };
        let section = |title: String| {
            div()
                .pt_0p5()
                .pl(px(INDENT - 4.))
                .text_size(px(10.))
                .text_color(colors.text_muted)
                .child(title)
                .into_any_element()
        };
        // Element ids only need to be unique within the tree.
        let prefix = format!(
            "{}-{}-{}",
            key.0,
            key.1.schema.as_deref().unwrap_or(""),
            key.1.name
        );
        let mut items = vec![section(format!("COLUMNS ({})", details.columns.len()))];
        items.extend(details.columns.iter().enumerate().map(|(i, c)| {
            entry(
                format!("col-{prefix}-{i}"),
                &c.name,
                c.summary(),
                c.primary_key,
            )
        }));
        if !details.indexes.is_empty() {
            items.push(section(format!("INDEXES ({})", details.indexes.len())));
            items.extend(
                details.indexes.iter().enumerate().map(|(i, x)| {
                    entry(format!("idx-{prefix}-{i}"), &x.name, x.summary(), x.primary)
                }),
            );
        }
        v_flex().pb_1().children(items).into_any_element()
    }
}

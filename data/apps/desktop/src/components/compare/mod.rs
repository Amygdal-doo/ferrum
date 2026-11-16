//! Schema compare tab: pick the target connection, see the differences per
//! table and the migration script (copy it or open it in a SQL tab).

use crate::app::FerrumData;
use crate::state::CompareLoad;
use ferrum_ui::gpui::ClipboardItem;
use ferrum_ui::prelude::*;
use ferrumdata_domain::EditorTabId;

impl FerrumData {
    pub fn render_compare_view(
        &mut self,
        tab_id: EditorTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let Some(source) = self
            .state
            .tabs
            .iter()
            .find(|t| t.id == tab_id)
            .and_then(|t| t.profile_id)
        else {
            return div().into_any_element();
        };
        let view = self
            .state
            .compares
            .get(&tab_id)
            .cloned()
            .unwrap_or_default();
        let source_name = self
            .state
            .profile(source)
            .map_or_else(String::new, |p| p.display_name.clone());
        let targets = self.compare_targets(source);
        let target_name = view
            .target
            .and_then(|id| targets.iter().find(|(t, _)| *t == id))
            .map_or_else(|| "Choose a connection".to_owned(), |(_, n)| n.clone());
        let this = cx.entity().downgrade();
        let choices = targets.clone();
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            for (id, name) in choices {
                let this = this.clone();
                menu = menu.entry(name, None, move |_, cx| {
                    if let Err(e) =
                        this.update(cx, |this, cx| this.set_compare_target(tab_id, id, cx))
                    {
                        tracing::debug!(error = %e, "compare tab dropped");
                    }
                });
            }
            menu
        });
        let loading = view.load == CompareLoad::Loading;

        let header = h_flex()
            .flex_none()
            .gap_2()
            .px_4()
            .py_2()
            .border_b_1()
            .border_color(colors.border_variant)
            .child(Label::new(format!("Source: {source_name}")).size(LabelSize::Small))
            .child(div().text_color(colors.text_muted).child("→ target:"))
            .child(DropdownMenu::new("compare-target", target_name, menu))
            .child(
                Button::new("compare-run", "Compare again")
                    .style(ButtonStyle::Outlined)
                    .disabled(view.target.is_none() || loading)
                    .on_click(cx.listener(move |this, _, _, cx| this.run_compare(tab_id, cx))),
            )
            .child(div().flex_1())
            .child(
                Label::new("The script makes the target match the source; it is never run here.")
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
            );
        let message = |text: String| {
            div()
                .p_4()
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                .into_any_element()
        };
        let body = match &view.load {
            CompareLoad::Idle if targets.is_empty() => {
                message("No other connection of the same engine to compare with.".into())
            }
            CompareLoad::Idle => message("Choose the target connection.".into()),
            CompareLoad::Loading => message("Reading both schemas…".into()),
            CompareLoad::Failed(e) => message(e.clone()),
            CompareLoad::Loaded { diff, script } => {
                let summary = if diff.is_empty() {
                    format!("The schemas match ({} tables).", diff.identical)
                } else {
                    format!(
                        "{} tables differ · {} identical",
                        diff.tables.len(),
                        diff.identical
                    )
                };
                let tables = diff.tables.iter().map(|t| {
                    let name = match &t.table.schema {
                        Some(s) => format!("{s}.{}", t.table.name),
                        None => t.table.name.clone(),
                    };
                    v_flex()
                        .gap_0p5()
                        .child(
                            div()
                                .text_sm()
                                .font_weight(ferrum_ui::gpui::FontWeight::SEMIBOLD)
                                .child(name),
                        )
                        .children(t.changes.iter().map(|change| {
                            let (mark, text) = change.describe();
                            let color = match mark {
                                '+' => status.created,
                                '−' => status.deleted,
                                _ => status.modified,
                            };
                            h_flex()
                                .gap_2()
                                .pl_2()
                                .text_xs()
                                .child(div().w(px(10.)).text_color(color).child(mark.to_string()))
                                .child(text)
                        }))
                });
                let copy = script.clone();
                h_flex()
                    .size_full()
                    .items_start()
                    .child(
                        v_flex()
                            .id("compare-diff")
                            .w(px(360.))
                            .flex_none()
                            .h_full()
                            .overflow_y_scroll()
                            .p_4()
                            .gap_3()
                            .border_r_1()
                            .border_color(colors.border_variant)
                            .child(Label::new(summary).size(LabelSize::Small))
                            .children(tables),
                    )
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .h_full()
                            .child(
                                h_flex()
                                    .gap_2()
                                    .px_4()
                                    .py_2()
                                    .child(Label::new("Migration script").size(LabelSize::Small))
                                    .child(div().flex_1())
                                    .child(
                                        Button::new("compare-copy", "Copy")
                                            .style(ButtonStyle::Outlined)
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                cx.write_to_clipboard(ClipboardItem::new_string(
                                                    copy.clone(),
                                                ));
                                                this.notify_user("Migration script copied.", false, cx);
                                            })),
                                    )
                                    .child(
                                        Button::new("compare-open", "Open in SQL tab")
                                            .style(ButtonStyle::Outlined)
                                            .disabled(diff.is_empty())
                                            .tooltip(Tooltip::text(
                                                "A new tab on the target connection, to review and run yourself",
                                            ))
                                            .on_click(cx.listener(move |this, _, _, cx| {
                                                this.open_migration(tab_id, cx);
                                            })),
                                    ),
                            )
                            .child(
                                div()
                                    .id("compare-script")
                                    .flex_1()
                                    .min_h_0()
                                    .overflow_scroll()
                                    .mx_4()
                                    .mb_4()
                                    .p_3()
                                    .rounded_md()
                                    .bg(colors.editor_background)
                                    .font_buffer(cx)
                                    .text_xs()
                                    .whitespace_nowrap()
                                    .child(script.clone()),
                            ),
                    )
                    .into_any_element()
            }
        };
        v_flex()
            .flex_1()
            .min_h_0()
            .child(header)
            .child(div().flex_1().min_h_0().child(body))
            .into_any_element()
    }
}

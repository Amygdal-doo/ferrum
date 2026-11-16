//! CSV import tab: file, options, column mapping and a preview of the rows
//! and of the first INSERT, then Import (one transaction).

use super::{status_line, toggle};
use crate::app::FerrumData;
use crate::commands::transfer::set_header;
use crate::state::TaskStatus;
use ferrum_ui::prelude::*;
use ferrumdata_domain::EditorTabId;

/// Data rows shown in the preview.
const PREVIEW_ROWS: usize = 10;
/// Characters of the first INSERT shown.
const PREVIEW_SQL: usize = 600;

impl FerrumData {
    pub fn render_import_view(
        &mut self,
        tab_id: EditorTabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(view) = self.state.imports.get(&tab_id).cloned() else {
            return div().into_any_element();
        };
        let engine = self.state.active_profile().map(|p| p.engine);
        let read_only = self.state.active_profile().is_some_and(|p| p.read_only);
        let running = view.status == TaskStatus::Running;
        let columns = match &view.columns {
            Ok(columns) => columns.clone(),
            Err(e) => {
                return div()
                    .p_4()
                    .child(Label::new(e.clone()).color(Color::Muted))
                    .into_any_element();
            }
        };
        let headers: Vec<String> = match &view.csv {
            Some(csv) if view.plan.header => csv.records.first().cloned().unwrap_or_default(),
            Some(csv) => (1..=csv.records.first().map_or(0, Vec::len))
                .map(|i| format!("column {i}"))
                .collect(),
            None => Vec::new(),
        };
        let file = view
            .file
            .as_ref()
            .map_or("No file chosen".to_owned(), |p| p.display().to_string());

        // Mapping: one row per table column with a dropdown of CSV columns.
        let this = cx.entity().downgrade();
        let mapping: Vec<_> = columns
            .iter()
            .enumerate()
            .map(|(col, name)| {
                let current = view.plan.mapping.get(col).copied().flatten();
                let label = current
                    .and_then(|i| headers.get(i))
                    .cloned()
                    .unwrap_or_else(|| "— default —".into());
                let this = this.clone();
                let headers = headers.clone();
                let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
                    let choices = std::iter::once((None, "— default —".to_owned())).chain(
                        headers
                            .iter()
                            .enumerate()
                            .map(|(i, h)| (Some(i), h.clone())),
                    );
                    for (choice, text) in choices {
                        let this = this.clone();
                        menu = menu.toggleable_entry(
                            text,
                            choice == current,
                            IconPosition::Start,
                            None,
                            move |_, cx| {
                                if let Err(e) = this.update(cx, |this, cx| {
                                    if let Some(slot) = this
                                        .state
                                        .imports
                                        .get_mut(&tab_id)
                                        .and_then(|v| v.plan.mapping.get_mut(col))
                                    {
                                        *slot = choice;
                                        cx.notify();
                                    }
                                }) {
                                    tracing::debug!(error = %e, "import tab dropped");
                                }
                            },
                        );
                    }
                    menu
                });
                h_flex()
                    .gap_2()
                    .child(div().w(px(180.)).truncate().text_sm().child(name.clone()))
                    .child(div().text_color(colors.text_muted).child("←"))
                    .child(DropdownMenu::new(
                        SharedString::from(format!("map-{tab_id}-{col}")),
                        label,
                        menu,
                    ))
            })
            .collect();

        let preview = view.csv.as_ref().map(|csv| {
            let data = view.plan.data(csv);
            let mapped: Vec<(usize, &String)> = view
                .plan
                .mapping
                .iter()
                .zip(&columns)
                .filter_map(|(m, c)| Some(((*m)?, c)))
                .collect();
            let cell = |text: String| div().w(px(140.)).flex_none().px_2().truncate().child(text);
            let header = h_flex()
                .border_b_1()
                .border_color(colors.border_variant)
                .text_color(colors.text_muted)
                .children(mapped.iter().map(|(_, c)| cell((*c).clone())));
            let rows = data.iter().take(PREVIEW_ROWS).map(|record| {
                h_flex().children(mapped.iter().map(|(i, _)| {
                    let value = record.get(*i).cloned();
                    match value {
                        Some(v) if v.is_empty() && view.plan.empty_as_null => cell("NULL".into()).text_color(colors.text_muted),
                        Some(v) => cell(v),
                        None => cell("(missing)".into()).text_color(cx.theme().status().error),
                    }
                }))
            });
            let sql = engine.map(|e| match view.plan.statements(e, &view.table, &columns, csv) {
                Ok(script) => {
                    let first: String = script
                        .first()
                        .map(|s| s.sql.chars().take(PREVIEW_SQL).collect())
                        .unwrap_or_default();
                    (
                        format!(
                            "{} rows in {} INSERT statements, one transaction (all or nothing). First statement:",
                            data.len(),
                            script.len()
                        ),
                        first,
                        None,
                    )
                }
                Err(e) => (String::new(), String::new(), Some(e)),
            });
            v_flex()
                .gap_2()
                .child(Label::new(format!("Preview (first {} of {} rows)", PREVIEW_ROWS.min(data.len()), data.len())).size(LabelSize::Small))
                .child(
                    v_flex()
                        .id("import-preview")
                        .overflow_x_scroll()
                        .font_buffer(cx)
                        .text_xs()
                        .child(header)
                        .children(rows),
                )
                .when_some(sql, |this, (summary, first, error)| {
                    this.when_some(error, |this, e| {
                        this.child(Label::new(e).size(LabelSize::Small).color(Color::Error))
                    })
                    .when(!summary.is_empty(), |this| {
                        this.child(Label::new(summary).size(LabelSize::Small).color(Color::Muted))
                            .child(
                                div()
                                    .p_2()
                                    .rounded_md()
                                    .bg(colors.editor_background)
                                    .font_buffer(cx)
                                    .text_xs()
                                    .child(first),
                            )
                    })
                })
        });
        let can_import = !running
            && !read_only
            && view.csv.is_some()
            && view.plan.mapping.iter().any(Option::is_some);

        v_flex()
            .id("import-view")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_4()
            .gap_3()
            .child(
                h_flex()
                    .gap_3()
                    .child(
                        Button::new("import-file", "Choose CSV file…")
                            .style(ButtonStyle::Outlined)
                            .disabled(running)
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.choose_import_file(tab_id, cx)
                            })),
                    )
                    .child(div().min_w_0().truncate().text_sm().child(file))
                    .when_some(view.csv.as_ref(), |this, csv| {
                        let delimiter = match csv.delimiter {
                            '\t' => "tab".to_owned(),
                            d => format!("“{d}”"),
                        };
                        this.child(
                            Label::new(format!("delimiter {delimiter}"))
                                .size(LabelSize::Small)
                                .color(Color::Muted),
                        )
                    }),
            )
            .child(
                h_flex()
                    .gap_4()
                    .child(
                        ferrum_ui::components::Checkbox::new(
                            "import-header",
                            toggle(view.plan.header),
                        )
                        .label("First row is a header")
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(
                            move |this, state: &ToggleState, _, cx| {
                                if let Some(view) = this.state.imports.get_mut(&tab_id) {
                                    set_header(view, state.selected());
                                    cx.notify();
                                }
                            },
                        )),
                    )
                    .child(
                        ferrum_ui::components::Checkbox::new(
                            "import-null",
                            toggle(view.plan.empty_as_null),
                        )
                        .label("Empty fields are NULL")
                        .label_size(LabelSize::Small)
                        .on_click(cx.listener(
                            move |this, state: &ToggleState, _, cx| {
                                if let Some(view) = this.state.imports.get_mut(&tab_id) {
                                    view.plan.empty_as_null = state.selected();
                                    cx.notify();
                                }
                            },
                        )),
                    ),
            )
            .when(view.csv.is_some(), |this| {
                this.child(
                    Label::new("Columns (unmapped columns get their default)")
                        .size(LabelSize::Small),
                )
                .child(v_flex().gap_1().children(mapping))
            })
            .children(preview)
            .when(read_only, |this| {
                this.child(
                    Label::new("Read-only connection: importing is disabled.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .children(status_line(&view.status, "Importing…", cx))
            .child(
                h_flex().child(
                    Button::new("import-run", "Import")
                        .style(ButtonStyle::Filled)
                        .disabled(!can_import)
                        .on_click(cx.listener(move |this, _, _, cx| this.run_import(tab_id, cx))),
                ),
            )
            .into_any_element()
    }
}

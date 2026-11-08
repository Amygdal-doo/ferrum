//! Results panel: status line, error/empty states and the virtual grid.

mod grid;

use crate::app::FerrumData;
use crate::state::TabRun;
use ferrum_ui::prelude::*;
use ferrumdata_domain::{
    CellValue, ColumnMeta, EditorTabId, ExecutionId, LogicalType, ProfileId, QueryState,
};

impl FerrumData {
    pub fn render_results(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(tab_id) = self.state.active_tab else {
            return div().into_any_element();
        };
        let warning = cx.theme().status().warning;
        let status_line = self.state.runs.get(&tab_id).map(|run| {
            h_flex()
                .gap_3()
                .child(
                    div()
                        .text_color(colors.text_muted)
                        .child(run.status_label()),
                )
                .when_some(run.note.clone(), |this, note| {
                    this.child(div().text_color(warning).child(note))
                })
                .when(run.buffer.is_truncated(), |this| {
                    this.child(div().text_color(warning).child(format!(
                        "Showing the first {} rows (client fetch limit). The SQL was not modified.",
                        run.buffer.len()
                    )))
                })
        });
        let body = if self.state.runs.contains_key(&tab_id) {
            self.render_run(tab_id, cx)
        } else {
            self.render_empty(tab_id, cx)
        };

        v_flex()
            .size_full()
            .min_h_0()
            .bg(colors.editor_background)
            .child(
                h_flex()
                    .flex_none()
                    .h(px(32.))
                    .gap_3()
                    .px_3()
                    .text_xs()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(
                        div()
                            .font_weight(ferrum_ui::gpui::FontWeight::MEDIUM)
                            .child("Results"),
                    )
                    .children(status_line),
            )
            .child(div().relative().flex_1().min_h_0().child(body))
            .into_any_element()
    }

    fn render_run(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        let Some(run) = self.state.runs.get(&tab_id) else {
            return div().into_any_element();
        };
        let message = |text: String| {
            div()
                .p_4()
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                .into_any_element()
        };
        if let Some(err) = &run.error {
            let line = |text: String| div().mt_1().child(text);
            return v_flex()
                .id("run-error")
                .size_full()
                .overflow_y_scroll()
                .p_4()
                .text_sm()
                .child(
                    div()
                        .font_weight(ferrum_ui::gpui::FontWeight::MEDIUM)
                        .text_color(cx.theme().status().error)
                        .child(err.message.clone()),
                )
                .when_some(err.code.clone(), |this, code| {
                    this.child(
                        div()
                            .mt_1()
                            .text_xs()
                            .text_color(cx.theme().colors().text_muted)
                            .child(format!("SQLSTATE {code}")),
                    )
                })
                .when_some(err.detail.clone(), |this, d| {
                    this.child(line(format!("Detail: {d}")))
                })
                .when_some(err.hint.clone(), |this, h| {
                    this.child(line(format!("Hint: {h}")))
                })
                .when_some(err.suggestion(), |this, s| {
                    this.child(
                        div()
                            .mt_2()
                            .text_color(cx.theme().colors().text_muted)
                            .child(s),
                    )
                })
                .into_any_element();
        }
        match run.state {
            QueryState::Created | QueryState::Running if run.buffer.columns().is_empty() => {
                message("Running…".into())
            }
            QueryState::Cancelled if run.buffer.is_empty() => message("Query cancelled.".into()),
            QueryState::TimedOut if run.buffer.is_empty() => message("Query timed out.".into()),
            QueryState::Completed if run.buffer.columns().is_empty() => div()
                .p_4()
                .text_sm()
                .child(run.status_label())
                .into_any_element(),
            _ => self.render_grid(tab_id, cx),
        }
    }

    fn render_empty(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_3()
            .child(
                Label::new("Run a statement to see results here.")
                    .size(LabelSize::Small)
                    .color(Color::Muted),
            )
            .child(
                Button::new("synthetic", "Load 100k synthetic rows (grid benchmark)")
                    .style(ButtonStyle::Outlined)
                    .tooltip(Tooltip::text(
                        "Grid spike (ADR-004): renders 100,000 generated rows without a database",
                    ))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.runs.insert(tab_id, synthetic_run(100_000));
                        cx.notify();
                    })),
            )
            .into_any_element()
    }
}

/// Generated data for the grid spike; no database involved.
fn synthetic_run(rows: usize) -> TabRun {
    let col = |name: &str, ty: LogicalType| ColumnMeta {
        name: name.into(),
        db_type: format!("{ty:?}").to_lowercase(),
        logical_type: ty,
        nullable: Some(true),
        primary_key: false,
        editable: false,
    };
    let mut run = TabRun::new(
        ExecutionId::new(),
        ProfileId::new(),
        "-- synthetic".into(),
        rows,
    );
    run.apply(ferrumdata_domain::QueryEvent::Started);
    run.apply(ferrumdata_domain::QueryEvent::Metadata(vec![
        col("id", LogicalType::Integer),
        col("email", LogicalType::Text),
        col("score", LogicalType::Float),
        col("active", LogicalType::Boolean),
        col("note", LogicalType::Text),
    ]));
    let batch = (0..rows)
        .map(|i| {
            vec![
                CellValue::Signed(i as i64 + 1),
                CellValue::Text(format!("user{i}@example.com")),
                CellValue::Float((i % 1000) as f64 / 7.0),
                CellValue::Bool(i % 3 == 0),
                if i % 10 == 0 {
                    CellValue::Null
                } else {
                    CellValue::Text(String::new())
                },
            ]
        })
        .collect();
    run.apply(ferrumdata_domain::QueryEvent::Rows(batch));
    run.apply(ferrumdata_domain::QueryEvent::Completed(
        ferrumdata_domain::QueryOutcome {
            rows_returned: rows,
            rows_affected: None,
            truncated: false,
            elapsed: std::time::Duration::ZERO,
        },
    ));
    run
}

#[cfg(test)]
mod tests {
    use super::synthetic_run;

    #[test]
    fn synthetic_run_completes_with_all_rows() {
        let run = synthetic_run(1_000);
        assert!(run.state.is_terminal());
        assert_eq!(run.buffer.len(), 1_000);
        assert_eq!(run.buffer.columns().len(), 5);
        assert_eq!(run.col_widths.len(), 5);
    }
}

//! Plan pane: the EXPLAIN tree as an indented table with cost, rows and
//! (after ANALYZE) actual rows and times; slow nodes are tinted.

use crate::app::FerrumData;
use crate::state::PlanLoad;
use ferrum_ui::prelude::*;
use ferrumdata_application::explain::{Plan, PlanRow};
use ferrumdata_domain::EditorTabId;

const INDENT: f32 = 16.0;
/// Numeric columns: header and width.
const COLUMNS: [(&str, f32); 6] = [
    ("Cost", 90.0),
    ("Rows", 80.0),
    ("Actual rows", 90.0),
    ("Loops", 60.0),
    ("Time ms", 80.0),
    ("Self ms", 80.0),
];

/// Compact number: integers without decimals, others to two places.
fn number(value: Option<f64>) -> String {
    match value {
        None => String::new(),
        Some(v) if v.fract() == 0.0 && v.abs() < 1e15 => format!("{v:.0}"),
        Some(v) => format!("{v:.2}"),
    }
}

impl FerrumData {
    pub fn render_plan(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        let message = |text: String| {
            div()
                .p_4()
                .child(Label::new(text).size(LabelSize::Small).color(Color::Muted))
                .into_any_element()
        };
        match self.state.plans.get(&tab_id) {
            None => message("Explain shows how the database will run the statement.".into()),
            Some(PlanLoad::Loading { analyze: true }) => message("Running EXPLAIN ANALYZE…".into()),
            Some(PlanLoad::Loading { analyze: false }) => message("Explaining…".into()),
            Some(PlanLoad::Failed(e)) => div()
                .p_4()
                .text_sm()
                .text_color(cx.theme().status().error)
                .child(e.clone())
                .into_any_element(),
            Some(PlanLoad::Loaded { plan, sql }) => {
                let (plan, sql) = (plan.clone(), sql.clone());
                render_plan_table(&plan, &sql, cx)
            }
        }
    }
}

fn render_plan_table(plan: &Plan, sql: &str, cx: &App) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let status = cx.theme().status().clone();
    let slow = plan.rows.iter().filter(|r| r.slow).count();
    let mut summary = vec![if plan.analyzed {
        "Analyzed (the statement was executed)".to_owned()
    } else {
        "Estimated plan".to_owned()
    }];
    if let Some(ms) = plan.execution_ms {
        summary.push(format!("execution {ms:.2} ms"));
    }
    if let Some(ms) = plan.planning_ms {
        summary.push(format!("planning {ms:.2} ms"));
    }
    if slow > 0 {
        summary.push(format!(
            "{slow} slow node{}",
            if slow == 1 { "" } else { "s" }
        ));
    }
    let header = h_flex()
        .px_2()
        .py_1()
        .border_b_1()
        .border_color(colors.border_variant)
        .text_color(colors.text_muted)
        .child(div().flex_1().child("Operation"))
        .children(
            COLUMNS
                .iter()
                .map(|(name, w)| div().flex_none().w(px(*w)).text_right().child(*name)),
        );
    let rows = plan.rows.iter().map(|row| plan_row(row, cx));

    v_flex()
        .id("plan")
        .size_full()
        .overflow_y_scroll()
        .text_xs()
        .child(
            h_flex()
                .gap_3()
                .px_2()
                .py_1p5()
                .child(div().flex_none().child(summary.join(" · ")))
                .child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_color(colors.text_muted)
                        .font_buffer(cx)
                        .child(sql.to_owned()),
                ),
        )
        .child(header)
        .children(rows)
        .when(slow > 0, |this| {
            this.child(
                div()
                    .px_2()
                    .py_1()
                    .text_color(status.warning)
                    .child(if plan.analyzed {
                        "Tinted: nodes taking at least 20 % of the time on their own."
                    } else {
                        "Tinted: full scans and temporary sorts, or nodes with at least 20 % of the estimated cost on their own."
                    }),
            )
        })
        .into_any_element()
}

fn plan_row(row: &PlanRow, cx: &App) -> impl IntoElement {
    let colors = cx.theme().colors().clone();
    let status = cx.theme().status().clone();
    let node = &row.node;
    let values = [
        node.cost,
        node.rows,
        node.actual_rows,
        node.loops,
        node.total_ms,
        row.self_ms,
    ];
    h_flex()
        .items_start()
        .px_2()
        .py_0p5()
        .border_b_1()
        .border_color(colors.border_variant)
        .when(row.slow, |this| this.bg(status.warning_background))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .pl(px(INDENT * row.depth as f32))
                .child(
                    h_flex()
                        .gap_1()
                        .when(row.depth > 0, |this| {
                            this.child(div().text_color(colors.text_muted).child("└"))
                        })
                        .when(row.slow, |this| {
                            this.child(div().text_color(status.warning).child("⚠"))
                        })
                        .child(div().truncate().child(node.label.clone())),
                )
                .when_some(node.detail.clone(), |this, detail| {
                    this.child(
                        div()
                            .pl(px(INDENT))
                            .truncate()
                            .text_color(colors.text_muted)
                            .font_buffer(cx)
                            .child(detail),
                    )
                }),
        )
        .children(COLUMNS.iter().zip(values).map(|((_, w), v)| {
            div()
                .flex_none()
                .w(px(*w))
                .text_right()
                .font_buffer(cx)
                .child(number(v))
        }))
}

#[cfg(test)]
mod tests {
    use super::number;

    #[test]
    fn numbers_are_compact() {
        assert_eq!(number(None), "");
        assert_eq!(number(Some(3.0)), "3");
        assert_eq!(number(Some(0.256)), "0.26");
    }
}

//! The response's Tests tab: test results, script errors, `log` output and
//! the variables the scripts set.

use crate::app::FerrumApi;
use crate::components::hint;
use ferrum_ui::components::{
    ActiveTheme as _, Color, Label, LabelCommon as _, LabelSize, StyledTypography as _, h_flex,
    v_flex,
};
use ferrumapi_domain::script::{ScriptReport, VarScope};
use gpui::{AnyElement, Context, div, prelude::*};

fn heading(text: String) -> impl IntoElement {
    Label::new(text).size(LabelSize::Small).color(Color::Muted)
}

pub fn tests_view(report: &ScriptReport, cx: &mut Context<FerrumApi>) -> AnyElement {
    let status = cx.theme().status().clone();
    let colors = cx.theme().colors().clone();
    let mono = |text: String, cx: &Context<FerrumApi>| div().text_xs().font_buffer(cx).child(text);
    let mut body = v_flex().gap_3().p_3();
    if report.tests.is_empty() && report.errors.is_empty() {
        body = body.child(hint(
            "No tests ran. Add `assert` / `test` lines in the request's Scripts tab.",
        ));
    }
    if !report.tests.is_empty() {
        let rows = report.tests.iter().map(|t| {
            let (mark, color) = if t.passed {
                ("PASS", status.success)
            } else {
                ("FAIL", status.error)
            };
            h_flex()
                .gap_3()
                .items_start()
                .child(
                    div()
                        .flex_none()
                        .w_10()
                        .text_xs()
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .text_color(color)
                        .child(mark),
                )
                .child(
                    v_flex()
                        .min_w_0()
                        .child(
                            div()
                                .text_sm()
                                .text_color(colors.text)
                                .child(t.name.clone()),
                        )
                        .when(!t.message.is_empty(), |this| {
                            this.child(hint(t.message.clone()))
                        }),
                )
        });
        body = body.child(
            v_flex()
                .gap_1p5()
                .child(heading(format!(
                    "Tests: {} of {} passed",
                    report.passed_count(),
                    report.tests.len()
                )))
                .children(rows),
        );
    }
    if !report.errors.is_empty() {
        body = body.child(
            v_flex()
                .gap_1()
                .child(heading("Script errors".to_owned()))
                .children(report.errors.iter().map(|e| {
                    div()
                        .text_xs()
                        .font_buffer(cx)
                        .text_color(status.error)
                        .child(e.clone())
                })),
        );
    }
    if !report.changes.is_empty() {
        body = body.child(
            v_flex()
                .gap_1()
                .child(heading("Variables set".to_owned()))
                .children(report.changes.iter().map(|c| {
                    let scope = match c.scope {
                        VarScope::Environment => "env",
                        VarScope::Global => "global",
                        VarScope::Run => "var",
                    };
                    let text = match &c.value {
                        Some(v) => format!("{scope}.{} = {v}", c.key),
                        None => format!("unset {scope}.{}", c.key),
                    };
                    mono(text, cx)
                })),
        );
    }
    if !report.logs.is_empty() {
        body = body.child(
            v_flex()
                .gap_1()
                .child(heading("Log".to_owned()))
                .children(report.logs.iter().map(|l| mono(l.clone(), cx))),
        );
    }
    div()
        .id("response-tests")
        .size_full()
        .overflow_y_scroll()
        .child(body)
        .into_any_element()
}

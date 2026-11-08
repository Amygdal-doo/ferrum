//! Virtualised grid: `uniform_list` only builds the rows in the viewport, so
//! element count is independent of result size (ADR-004).

use crate::app::FerrumData;
use ferrum_ui::gpui::{Hsla, ListSizingBehavior, uniform_list};
use ferrum_ui::prelude::*;
use ferrumdata_domain::{CellValue, EditorTabId};
use std::ops::Range;

const ROW_HEIGHT: f32 = 26.0;
const HEADER_HEIGHT: f32 = 40.0;
const FALLBACK_COL_WIDTH: f64 = 160.0;
const ROWNUM_WIDTH: f32 = 64.0;

impl FerrumData {
    pub fn render_grid(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(run) = self.state.runs.get(&tab_id) else {
            return div().into_any_element();
        };
        let buffer = &run.buffer;
        let total = buffer.len();
        let widths: Vec<f32> = (0..buffer.columns().len())
            .map(|i| run.col_widths.get(i).copied().unwrap_or(FALLBACK_COL_WIDTH) as f32)
            .collect();
        let width = ROWNUM_WIDTH + widths.iter().sum::<f32>();

        let header = h_flex()
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .w(px(width))
            .bg(colors.panel_background)
            .border_b_1()
            .border_color(colors.border)
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .w(px(ROWNUM_WIDTH))
                    .border_r_1()
                    .border_color(colors.border),
            )
            .children(buffer.columns().iter().zip(widths.iter()).enumerate().map(
                |(index, (col, w))| {
                    v_flex()
                        .id(SharedString::from(format!("col-{index}")))
                        .flex_none()
                        .h_full()
                        .w(px(*w))
                        .justify_center()
                        .overflow_hidden()
                        .px_2()
                        .border_r_1()
                        .border_color(colors.border)
                        .tooltip(Tooltip::text(format!("{} ({})", col.name, col.db_type)))
                        .child(div().truncate().text_xs().child(col.name.clone()))
                        .child(
                            div()
                                .truncate()
                                .text_size(px(10.))
                                .text_color(colors.text_muted)
                                .child(col.db_type.to_lowercase()),
                        )
                },
            ));

        let rows = uniform_list(
            "result-rows",
            total,
            cx.processor(move |this, range: Range<usize>, _, cx| {
                this.render_rows(tab_id, range, cx)
            }),
        )
        .with_sizing_behavior(ListSizingBehavior::Infer)
        .flex_1()
        .w(px(width))
        .track_scroll(&self.grid_scroll);

        // Horizontal scrolling lives on the outer container so the header
        // moves with the rows; the list itself only scrolls vertically.
        div()
            .id("result-grid")
            .absolute()
            .inset_0()
            .overflow_x_scroll()
            .font_buffer(cx)
            .text_xs()
            .child(v_flex().h_full().w(px(width)).child(header).child(rows))
            .into_any_element()
    }

    fn render_rows(
        &mut self,
        tab_id: EditorTabId,
        range: Range<usize>,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let Some(run) = self.state.runs.get(&tab_id) else {
            return Vec::new();
        };
        let buffer = &run.buffer;
        let widths: Vec<f32> = (0..buffer.columns().len())
            .map(|i| run.col_widths.get(i).copied().unwrap_or(FALLBACK_COL_WIDTH) as f32)
            .collect();
        let numeric: Vec<bool> = buffer
            .columns()
            .iter()
            .map(|m| m.logical_type.is_numeric())
            .collect();

        buffer
            .rows(range)
            .map(|(index, row)| {
                h_flex()
                    .h(px(ROW_HEIGHT))
                    .border_b_1()
                    .border_color(colors.border_variant)
                    .hover(|s| s.bg(colors.element_hover))
                    .child(
                        div()
                            .flex_none()
                            .h_full()
                            .w(px(ROWNUM_WIDTH))
                            .px_2()
                            .flex()
                            .items_center()
                            .justify_end()
                            .bg(colors.panel_background)
                            .border_r_1()
                            .border_color(colors.border)
                            .text_color(colors.text_muted)
                            .child((index + 1).to_string()),
                    )
                    .children(row.iter().enumerate().map(|(c, cell)| {
                        let tone: Option<Hsla> = match cell {
                            CellValue::Null | CellValue::Unsupported(_) => Some(colors.text_muted),
                            CellValue::Bool(_) => Some(status.info),
                            _ => None,
                        };
                        // Rendered as plain text: database values are never interpreted.
                        div()
                            .flex_none()
                            .h_full()
                            .w(px(widths
                                .get(c)
                                .copied()
                                .unwrap_or(FALLBACK_COL_WIDTH as f32)))
                            .px_2()
                            .flex()
                            .items_center()
                            .when(numeric.get(c).copied().unwrap_or(false), |this| {
                                this.justify_end()
                            })
                            .overflow_hidden()
                            .border_r_1()
                            .border_color(colors.border_variant)
                            .when_some(tone, |this, color| this.text_color(color))
                            .when(
                                matches!(cell, CellValue::Null | CellValue::Unsupported(_)),
                                |this| this.italic(),
                            )
                            .child(div().truncate().child(cell.display()))
                    }))
                    .into_any_element()
            })
            .collect()
    }
}

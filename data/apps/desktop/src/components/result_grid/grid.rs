//! Virtualised grid: `uniform_list` only builds the rows in the viewport, so
//! element count is independent of result size (ADR-004). Cells select on click
//! (shift extends) and copy with ⌘C; headers live in `header.rs`.

use crate::app::FerrumData;
use ferrum_ui::gpui::{
    ClipboardItem, Hsla, ListSizingBehavior, MouseButton, MouseDownEvent, uniform_list,
};
use ferrum_ui::prelude::*;
use ferrumdata_domain::{CellValue, EditorTabId};
use ferrumdata_result_grid_core::GridSelection;
use std::ops::Range;

const ROW_HEIGHT: f32 = 26.0;
const FALLBACK_COL_WIDTH: f64 = 160.0;
pub(super) const ROWNUM_WIDTH: f32 = 64.0;
const KEY_CONTEXT: &str = "ResultGrid";

actions!(result_grid, [CopySelection, SelectAllCells]);

/// Grid shortcuts; active while the grid has focus (after a click into it).
pub fn bind_grid_keys(cx: &mut App) {
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-c", CopySelection, context),
        KeyBinding::new("ctrl-c", CopySelection, context),
        KeyBinding::new("cmd-a", SelectAllCells, context),
        KeyBinding::new("ctrl-a", SelectAllCells, context),
    ]);
}

impl FerrumData {
    pub fn render_grid(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) -> AnyElement {
        let Some(run) = self.state.runs.get(&tab_id) else {
            return div().into_any_element();
        };
        let buffer = &run.buffer;
        let total = buffer.len();
        let widths = column_widths(&run.col_widths, buffer.columns().len());
        let width = ROWNUM_WIDTH + widths.iter().sum::<f32>();

        let header = self.render_grid_header(tab_id, &widths, width, cx);

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
            .track_focus(&self.grid_focus)
            .key_context(KEY_CONTEXT)
            .on_action(cx.listener(move |this, _: &CopySelection, _, cx| {
                this.copy_selection(tab_id, cx);
            }))
            .on_action(cx.listener(move |this, _: &SelectAllCells, _, cx| {
                if let Some(run) = this.state.runs.get_mut(&tab_id) {
                    run.selection =
                        GridSelection::all(run.buffer.len(), run.buffer.columns().len());
                    cx.notify();
                }
            }))
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
        let widths = column_widths(&run.col_widths, run.buffer.columns().len());
        let numeric: Vec<bool> = run
            .buffer
            .columns()
            .iter()
            .map(|m| m.logical_type.is_numeric())
            .collect();
        let selection = run.selection;

        run.view_rows(range)
            .map(|(index, row)| {
                let row_selected =
                    selection.is_some_and(|s| s.whole_rows && s.rows().contains(&index));
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
                            .bg(if row_selected {
                                colors.element_selected
                            } else {
                                colors.panel_background
                            })
                            .border_r_1()
                            .border_color(colors.border)
                            .text_color(colors.text_muted)
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                    this.select(tab_id, index, None, e.modifiers.shift, cx);
                                }),
                            )
                            .child((index + 1).to_string()),
                    )
                    .children(row.iter().enumerate().map(|(c, cell)| {
                        let tone: Option<Hsla> = match cell {
                            CellValue::Null | CellValue::Unsupported(_) => Some(colors.text_muted),
                            CellValue::Bool(_) => Some(status.info),
                            _ => None,
                        };
                        let selected = selection.is_some_and(|s| s.contains(index, c));
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
                            .when(selected, |this| this.bg(colors.element_selected))
                            .when_some(tone, |this, color| this.text_color(color))
                            .when(
                                matches!(cell, CellValue::Null | CellValue::Unsupported(_)),
                                |this| this.italic(),
                            )
                            .on_mouse_down(
                                MouseButton::Left,
                                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                    this.select(tab_id, index, Some(c), e.modifiers.shift, cx);
                                }),
                            )
                            .child(div().truncate().child(cell.display()))
                    }))
                    .into_any_element()
            })
            .collect()
    }

    /// Click selects a cell (or, with `col: None`, a row); shift-click extends.
    fn select(
        &mut self,
        tab_id: EditorTabId,
        row: usize,
        col: Option<usize>,
        extend: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(run) = self.state.runs.get_mut(&tab_id) else {
            return;
        };
        run.selection = Some(match (run.selection, col) {
            (Some(s), Some(c)) if extend && !s.whole_rows => s.extend_to(row, c),
            (Some(s), None) if extend && s.whole_rows => s.extend_to(row, 0),
            (_, Some(c)) => GridSelection::cell(row, c),
            (_, None) => GridSelection::row(row),
        });
        cx.notify();
    }

    fn copy_selection(&mut self, tab_id: EditorTabId, cx: &mut Context<Self>) {
        if let Some(text) = self.state.runs.get(&tab_id).and_then(|r| r.copy_text()) {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
        }
    }
}

pub(super) fn column_widths(widths: &[f64], count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| widths.get(i).copied().unwrap_or(FALLBACK_COL_WIDTH) as f32)
        .collect()
}

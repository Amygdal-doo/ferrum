//! Grid header: click a column to sort it, drag its right edge to resize.

use super::grid::{ROWNUM_WIDTH, column_widths};
use crate::app::FerrumData;
use ferrum_ui::gpui::CursorStyle;
use ferrum_ui::prelude::*;
use ferrumdata_domain::EditorTabId;
use ferrumdata_result_grid_core::SortDirection;

const HEADER_HEIGHT: f32 = 40.0;
const RESIZE_HANDLE_WIDTH: f32 = 6.0;

/// Drag payload of a column's resize handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ColumnResize {
    pub col: usize,
}

impl Render for ColumnResize {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ferrum_ui::gpui::Empty
    }
}

impl FerrumData {
    pub(super) fn render_grid_header(
        &mut self,
        tab_id: EditorTabId,
        widths: &[f32],
        width: f32,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let Some(run) = self.state.runs.get(&tab_id) else {
            return div().into_any_element();
        };
        let sort = run.sort;
        h_flex()
            .id("grid-header")
            .flex_none()
            .h(px(HEADER_HEIGHT))
            .w(px(width))
            .bg(colors.panel_background)
            .border_b_1()
            .border_color(colors.border)
            .on_drag_move(
                cx.listener(move |this, event: &DragMoveEvent<ColumnResize>, _, cx| {
                    let col = event.drag(cx).col;
                    let Some(run) = this.state.runs.get_mut(&tab_id) else {
                        return;
                    };
                    let widths = column_widths(&run.col_widths, run.buffer.columns().len());
                    let left = event.bounds.left()
                        + px(ROWNUM_WIDTH + widths.iter().take(col).sum::<f32>());
                    run.resize_column(col, f64::from(f32::from(event.event.position.x - left)));
                    cx.notify();
                }),
            )
            .child(
                div()
                    .flex_none()
                    .h_full()
                    .w(px(ROWNUM_WIDTH))
                    .border_r_1()
                    .border_color(colors.border),
            )
            .children(
                run.buffer
                    .columns()
                    .iter()
                    .zip(widths.iter())
                    .enumerate()
                    .map(|(index, (col, w))| {
                        let arrow = match sort {
                            Some((c, SortDirection::Ascending)) if c == index => " ▲",
                            Some((c, SortDirection::Descending)) if c == index => " ▼",
                            _ => "",
                        };
                        h_flex()
                            .flex_none()
                            .h_full()
                            .w(px(*w))
                            .border_r_1()
                            .border_color(colors.border)
                            .child(
                                v_flex()
                                    .id(SharedString::from(format!("col-{index}")))
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .justify_center()
                                    .overflow_hidden()
                                    .pl_2()
                                    .cursor_pointer()
                                    .hover(|s| s.bg(colors.element_hover))
                                    .tooltip(Tooltip::text(format!(
                                        "{} ({}) · click to sort",
                                        col.name, col.db_type
                                    )))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if let Some(run) = this.state.runs.get_mut(&tab_id) {
                                            run.toggle_sort(index);
                                            cx.notify();
                                        }
                                    }))
                                    .child(
                                        div()
                                            .truncate()
                                            .text_xs()
                                            .child(format!("{}{arrow}", col.name)),
                                    )
                                    .child(
                                        div()
                                            .truncate()
                                            .text_size(px(10.))
                                            .text_color(colors.text_muted)
                                            .child(col.db_type.to_lowercase()),
                                    ),
                            )
                            .child(
                                div()
                                    .id(SharedString::from(format!("col-resize-{index}")))
                                    .flex_none()
                                    .h_full()
                                    .w(px(RESIZE_HANDLE_WIDTH))
                                    .cursor(CursorStyle::ResizeLeftRight)
                                    .hover(|s| s.bg(colors.border_focused))
                                    .on_drag(ColumnResize { col: index }, |payload, _, _, cx| {
                                        cx.new(|_| *payload)
                                    }),
                            )
                    }),
            )
            .into_any_element()
    }
}

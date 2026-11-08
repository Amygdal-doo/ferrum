//! Virtualised grid: only rows in the viewport (plus overscan) become DOM
//! nodes, so DOM size is independent of result size (ADR-004).

use crate::state::AppState;
use dioxus::prelude::*;
use ferrumdata_domain::{CellValue, EditorTabId};
use ferrumdata_result_grid_core::{Viewport, visible_range};

const ROW_HEIGHT: f64 = 26.0;
const HEADER_HEIGHT: f64 = 40.0;
const FALLBACK_COL_WIDTH: f64 = 160.0;
const ROWNUM_WIDTH: f64 = 64.0;

#[component]
pub fn VirtualGrid(tab_id: EditorTabId) -> Element {
    let state = use_context::<AppState>();
    let mut scroll_top = use_signal(|| 0.0f64);
    let mut height = use_signal(|| 400.0f64);
    let runs = state.runs.read();
    let Some(run) = runs.get(&tab_id) else {
        return rsx! {};
    };
    let buffer = &run.buffer;
    let total = buffer.len();
    let vp = Viewport {
        scroll_top: scroll_top(),
        height: height(),
        row_height: ROW_HEIGHT,
        overscan: 8,
    };
    let range = visible_range(&vp, total);
    let widths: Vec<f64> = (0..buffer.columns().len())
        .map(|i| run.col_widths.get(i).copied().unwrap_or(FALLBACK_COL_WIDTH))
        .collect();
    let width = ROWNUM_WIDTH + widths.iter().sum::<f64>();
    let content_height = HEADER_HEIGHT + vp.content_height(total);

    rsx! {
        div {
            class: "absolute inset-0 overflow-auto font-mono text-xs",
            role: "grid",
            "aria-rowcount": "{total}",
            onscroll: move |e| {
                scroll_top.set(e.data().scroll_top());
                height.set(f64::from(e.data().client_height()));
            },
            onresize: move |e| {
                if let Ok(size) = e.data().get_border_box_size() { height.set(size.height); }
            },
            div { class: "relative", style: "height: {content_height}px; width: {width}px;",
                // Header (sticky inside the scroll container).
                div {
                    class: "sticky top-0 z-10 flex border-b bg-sidenav font-sans",
                    style: "height: {HEADER_HEIGHT}px; width: {width}px;",
                    role: "row",
                    div { class: "shrink-0 border-r", style: "width: {ROWNUM_WIDTH}px" }
                    for (col, w) in buffer.columns().iter().zip(widths.iter()) {
                        div {
                            class: "flex shrink-0 flex-col justify-center overflow-hidden border-r px-2",
                            style: "width: {w}px",
                            role: "columnheader",
                            title: "{col.name} ({col.db_type})",
                            span { class: "truncate font-medium", "{col.name}" }
                            span { class: "truncate text-[10px] text-muted-foreground", "{col.db_type.to_lowercase()}" }
                        }
                    }
                }
                for (i, row) in buffer.rows(range) {
                    div {
                        key: "{i}",
                        class: "absolute left-0 flex border-b hover:bg-accent/50",
                        style: "top: {HEADER_HEIGHT + i as f64 * ROW_HEIGHT}px; height: {ROW_HEIGHT}px; width: {width}px;",
                        role: "row",
                        div {
                            class: "shrink-0 border-r bg-sidenav px-2 text-right leading-[26px] text-muted-foreground",
                            style: "width: {ROWNUM_WIDTH}px",
                            "{i + 1}"
                        }
                        for (c, cell) in row.iter().enumerate() {
                            Cell {
                                key: "{c}",
                                cell: cell.clone(),
                                width: widths.get(c).copied().unwrap_or(FALLBACK_COL_WIDTH),
                                numeric: buffer.columns().get(c).is_some_and(|m| m.logical_type.is_numeric()),
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn Cell(cell: CellValue, width: f64, numeric: bool) -> Element {
    let align = if numeric { "text-right" } else { "" };
    let tone = match &cell {
        CellValue::Null | CellValue::Unsupported(_) => "italic text-muted-foreground",
        CellValue::Bool(_) => "text-info-dark",
        _ => "",
    };
    // Rendered as a text node: database values are never interpreted as HTML.
    let text = cell.display();
    rsx! {
        div {
            class: "shrink-0 truncate border-r px-2 leading-[26px] select-text {align} {tone}",
            style: "width: {width}px",
            role: "gridcell",
            title: "{text}",
            "{text}"
        }
    }
}

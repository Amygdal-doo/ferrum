//! Results panel: status line, error/empty states and the virtual grid.

mod grid;

use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, TabRun};
use dioxus::prelude::*;
use ferrum_domain::{
    CellValue, ColumnMeta, EditorTabId, ExecutionId, LogicalType, ProfileId, QueryState,
};
use grid::VirtualGrid;

#[component]
pub fn ResultsPanel() -> Element {
    let state = use_context::<AppState>();
    let Some(tab) = state.active_tab() else {
        return rsx! {};
    };
    let runs = state.runs.read();
    let run = runs.get(&tab.id);

    rsx! {
        section { class: "flex h-full min-h-0 flex-col bg-background", "aria-label": "Results",
            div { class: "flex h-8 shrink-0 items-center gap-3 border-b px-3 text-xs",
                span { class: "font-medium", "Results" }
                if let Some(run) = run {
                    span { class: "text-muted-foreground", "{run.status_label()}" }
                    if let Some(note) = &run.note {
                        span { class: "text-warning-dark", "{note}" }
                    }
                    if run.buffer.is_truncated() {
                        span { class: "text-warning-dark",
                            "Showing the first {run.buffer.len()} rows (client fetch limit). The SQL was not modified."
                        }
                    }
                }
            }
            div { class: "relative min-h-0 flex-1",
                match run {
                    None => rsx! { EmptyState { tab_id: tab.id } },
                    Some(_) => rsx! { RunView { tab_id: tab.id } },
                }
            }
        }
    }
}

/// Takes the tab id, not the run: props are cloned and compared on every
/// render, which must never touch the row buffer.
#[component]
fn RunView(tab_id: EditorTabId) -> Element {
    let state = use_context::<AppState>();
    let runs = state.runs.read();
    let Some(run) = runs.get(&tab_id) else {
        return rsx! {};
    };
    if let Some(err) = &run.error {
        return rsx! {
            div { class: "h-full overflow-auto p-4 text-sm select-text",
                p { class: "font-medium text-destructive", "{err.message}" }
                if let Some(code) = &err.code { p { class: "mt-1 text-xs text-muted-foreground", "SQLSTATE {code}" } }
                if let Some(d) = &err.detail { p { class: "mt-2", "Detail: {d}" } }
                if let Some(h) = &err.hint { p { class: "mt-1", "Hint: {h}" } }
                if let Some(s) = err.suggestion() { p { class: "mt-2 text-muted-foreground", "{s}" } }
            }
        };
    }
    match run.state {
        QueryState::Created | QueryState::Running if run.buffer.columns().is_empty() => rsx! {
            div { class: "p-4 text-sm text-muted-foreground", "Running…" }
        },
        QueryState::Cancelled if run.buffer.is_empty() => rsx! {
            div { class: "p-4 text-sm text-muted-foreground", "Query cancelled." }
        },
        QueryState::TimedOut if run.buffer.is_empty() => rsx! {
            div { class: "p-4 text-sm text-muted-foreground", "Query timed out." }
        },
        QueryState::Completed if run.buffer.columns().is_empty() => rsx! {
            div { class: "p-4 text-sm", "{run.status_label()}" }
        },
        _ => rsx! { VirtualGrid { tab_id } },
    }
}

#[component]
fn EmptyState(tab_id: EditorTabId) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        div { class: "flex h-full flex-col items-center justify-center gap-3 text-sm text-muted-foreground",
            p { "Run a statement to see results here." }
            Button {
                size: ButtonSize::Sm,
                variant: ButtonVariant::Outline,
                title: "Grid spike (ADR-004): renders 100,000 generated rows without a database",
                onclick: move |_| { state.runs.write().insert(tab_id, synthetic_run(100_000)); },
                "Load 100k synthetic rows (grid benchmark)"
            }
        }
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
    run.apply(ferrum_domain::QueryEvent::Started);
    run.apply(ferrum_domain::QueryEvent::Metadata(vec![
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
    run.apply(ferrum_domain::QueryEvent::Rows(batch));
    run.apply(ferrum_domain::QueryEvent::Completed(
        ferrum_domain::QueryOutcome {
            rows_returned: rows,
            rows_affected: None,
            truncated: false,
            elapsed: std::time::Duration::ZERO,
        },
    ));
    run
}

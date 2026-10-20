//! Lazy table tree under each connection.

use crate::commands::schema;
use crate::state::{AppState, SchemaLoad};
use dioxus::prelude::*;
use ferrum_application::AppServices;
use ferrum_domain::{ProfileId, TableInfo, TableKind};

/// Max rows rendered in one tree; beyond this the user is asked to filter.
/// Keeps DOM size bounded for schemas with thousands of objects.
const MAX_RENDERED: usize = 500;

#[component]
pub fn TableTree(profile_id: ProfileId, filter: String) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let schemas = state.schemas.read();
    let svc_retry = services.clone();

    let tables = match schemas.get(&profile_id) {
        None | Some(SchemaLoad::Loading) => {
            return rsx! { div { class: "py-1 pl-7 text-[11px] text-muted-foreground", "Loading tables…" } };
        }
        Some(SchemaLoad::Failed(e)) => {
            return rsx! {
                div { class: "py-1 pl-7 text-[11px] text-destructive select-text",
                    "{e} "
                    button { class: "underline", onclick: move |_| schema::load_tables(state, svc_retry.clone(), profile_id), "Retry" }
                }
            };
        }
        Some(SchemaLoad::Loaded(t)) => t,
    };

    let needle = filter.to_lowercase();
    let matching: Vec<&TableInfo> = tables
        .iter()
        .filter(|t| needle.is_empty() || t.table.name.to_lowercase().contains(&needle))
        .collect();
    let hidden = matching.len().saturating_sub(MAX_RENDERED);
    let single_schema = tables
        .windows(2)
        .all(|w| w[0].table.schema == w[1].table.schema);
    let mut last_schema: Option<&str> = None;
    let mut items = Vec::new();
    for t in matching.iter().take(MAX_RENDERED) {
        let schema_name = t.table.schema.as_deref().unwrap_or("");
        if !single_schema && last_schema != Some(schema_name) {
            items.push((Some(schema_name.to_owned()), None));
            last_schema = Some(schema_name);
        }
        items.push((None, Some((*t).clone())));
    }
    let total = tables.len();

    rsx! {
        div { class: "pb-1",
            div { class: "flex items-center gap-1 py-0.5 pl-6 pr-1 text-[11px] text-muted-foreground",
                span { "Tables ({total})" }
                div { class: "flex-1" }
                button {
                    class: "rounded px-1 hover:bg-accent",
                    title: "Reload tables",
                    "aria-label": "Reload tables",
                    onclick: move |_| schema::load_tables(state, services.clone(), profile_id),
                    "⟳"
                }
            }
            if matching.is_empty() {
                div { class: "py-1 pl-7 text-[11px] text-muted-foreground", "No tables match." }
            }
            for (header, table) in items {
                if let Some(h) = header {
                    div { class: "pt-1 pl-6 text-[10px] uppercase tracking-wide text-muted-foreground", "{h}" }
                }
                if let Some(info) = table {
                    TableItem { key: "{info.table.schema:?}.{info.table.name}", profile_id, info }
                }
            }
            if hidden > 0 {
                div { class: "py-1 pl-7 text-[11px] text-muted-foreground", "{hidden} more — type in the filter to narrow down" }
            }
        }
    }
}

#[component]
fn TableItem(profile_id: ProfileId, info: TableInfo) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let active = state
        .active_tab()
        .is_some_and(|t| t.profile_id == Some(profile_id) && t.table.as_ref() == Some(&info.table));
    let cls = if active {
        "bg-accent text-foreground"
    } else {
        "text-foreground/80 hover:bg-accent/60"
    };
    let icon = match info.kind {
        TableKind::View | TableKind::MaterializedView => "◇",
        _ => "▦",
    };
    let rows = info
        .estimated_rows
        .map(|n| format!("~{n} rows"))
        .unwrap_or_default();
    let title = format!(
        "{} ({}) {rows}",
        info.table
            .qualified(ferrum_domain::DatabaseEngine::Postgres),
        info.kind.label()
    );
    let open = info.clone();

    rsx! {
        div {
            class: "flex cursor-default items-center gap-1.5 rounded py-0.5 pl-7 pr-2 text-xs {cls}",
            title: "{title}",
            role: "treeitem",
            onclick: move |_| schema::open_table(state, services.clone(), profile_id, open.clone()),
            span { class: "text-[10px] text-muted-foreground", "{icon}" }
            span { class: "truncate", "{info.table.name}" }
        }
    }
}

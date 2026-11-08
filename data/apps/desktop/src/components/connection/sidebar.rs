use crate::commands::{connection, schema};
use crate::components::env_dot_class;
use crate::components::explorer::TableTree;
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::SchemaLoad;
use crate::state::{AppState, ConnStatus, Dialog};
use dioxus::prelude::*;
use ferrum_application::AppServices;
use ferrum_domain::ConnectionProfile;

#[component]
pub fn Sidebar() -> Element {
    let mut state = use_context::<AppState>();
    let mut filter = use_signal(String::new);
    let needle = filter.read().to_lowercase();
    let schemas = state.schemas.read();
    let table_match = |p: &ConnectionProfile| match schemas.get(&p.id) {
        Some(SchemaLoad::Loaded(t)) => t
            .iter()
            .any(|t| t.table.name.to_lowercase().contains(&needle)),
        _ => false,
    };
    let profiles: Vec<ConnectionProfile> = state
        .profiles
        .read()
        .iter()
        .filter(|p| {
            needle.is_empty() || p.display_name.to_lowercase().contains(&needle) || table_match(p)
        })
        .cloned()
        .collect();
    drop(schemas);
    let empty = state.profiles.read().is_empty();

    rsx! {
        aside { class: "flex h-full flex-col bg-sidenav",
            div { class: "flex h-10 items-center justify-between px-3",
                span { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Connections" }
                Button {
                    size: ButtonSize::IconSm,
                    variant: ButtonVariant::Ghost,
                    aria_label: "New connection",
                    title: "New connection",
                    onclick: move |_| state.dialog.set(Some(Dialog::ConnectionForm(None))),
                    "+"
                }
            }
            if !empty {
                div { class: "px-3 pb-2",
                    input {
                        class: "h-7 w-full rounded-md border bg-background px-2 text-xs",
                        placeholder: "Filter connections and tables",
                        "aria-label": "Filter connections and tables",
                        value: "{filter}",
                        oninput: move |e| filter.set(e.value()),
                    }
                }
            }
            div { class: "min-h-0 flex-1 overflow-y-auto px-2 pb-2",
                if empty {
                    div { class: "px-2 py-6 text-center text-xs text-muted-foreground",
                        p { "No connections yet." }
                        Button {
                            class: "mt-3",
                            size: ButtonSize::Sm,
                            variant: ButtonVariant::Outline,
                            onclick: move |_| state.dialog.set(Some(Dialog::ConnectionForm(None))),
                            "Add PostgreSQL connection"
                        }
                    }
                }
                for p in profiles {
                    ProfileRow { key: "{p.id}", profile: p, filter: needle.clone() }
                }
            }
        }
    }
}

#[component]
fn ProfileRow(profile: ConnectionProfile, filter: String) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let status = state.conn_status.read().get(&profile.id).cloned();
    let active = state.active_tab().and_then(|t| t.profile_id) == Some(profile.id);
    let row_class = if active {
        "bg-accent"
    } else {
        "hover:bg-accent/60"
    };
    let (status_text, status_class) = match &status {
        None => ("", ""),
        Some(ConnStatus::Connecting) => ("connecting…", "text-muted-foreground"),
        Some(ConnStatus::Ready(_)) => ("connected", "text-success"),
        Some(ConnStatus::Failed(_)) => ("failed", "text-destructive"),
    };
    let error = match &status {
        Some(ConnStatus::Failed(m)) => Some(m.clone()),
        _ => None,
    };
    let connected = matches!(status, Some(ConnStatus::Ready(_)));
    let id = profile.id;
    let expanded = state.expanded.read().contains(&id);
    let chevron = if expanded { "▾" } else { "▸" };
    let svc_tree = services.clone();
    let (svc_a, svc_b) = (services.clone(), services.clone());
    let edit = profile.clone();
    let delete = profile.clone();

    rsx! {
        div { class: "group rounded-md px-2 py-1.5 {row_class}",
            div {
                class: "flex cursor-default items-center gap-2",
                title: "{profile.target_label()}",
                onclick: move |_| {
                    if let Some(tab_id) = *state.active_tab.peek() {
                        state.update_tab(tab_id, |t| { t.profile_id = Some(id); t.unsaved = true; });
                    }
                },
                ondoubleclick: move |_| connection::connect(state, svc_a.clone(), id),
                button {
                    class: "w-3 shrink-0 text-[10px] text-muted-foreground hover:text-foreground",
                    "aria-label": if expanded { "Collapse tables" } else { "Expand tables" },
                    "aria-expanded": expanded,
                    onclick: move |e| { e.stop_propagation(); schema::toggle(state, svc_tree.clone(), id); },
                    "{chevron}"
                }
                span { class: "size-2 shrink-0 rounded-full {env_dot_class(profile.env)}" }
                span { class: "truncate text-sm", "{profile.display_name}" }
                span { class: "ml-auto text-[10px] {status_class}", "{status_text}" }
            }
            div { class: "truncate pl-9 text-[11px] text-muted-foreground", "{profile.target_label()}" }
            if let Some(err) = error {
                div { class: "mt-1 pl-4 text-[11px] text-destructive select-text", "{err}" }
            }
            div { class: "mt-1 hidden gap-1 pl-3 group-hover:flex",
                if connected {
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-6 px-2 text-xs",
                        onclick: move |_| connection::disconnect(state, svc_b.clone(), id), "Disconnect" }
                } else {
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-6 px-2 text-xs",
                        onclick: move |_| connection::connect(state, svc_b.clone(), id), "Connect" }
                }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-6 px-2 text-xs",
                    onclick: move |_| state.dialog.set(Some(Dialog::ConnectionForm(Some(edit.clone())))), "Edit" }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-6 px-2 text-xs text-destructive",
                    onclick: move |_| state.dialog.set(Some(Dialog::DeleteProfile(delete.clone()))), "Delete" }
            }
            if expanded {
                TableTree { profile_id: id, filter }
            }
        }
    }
}

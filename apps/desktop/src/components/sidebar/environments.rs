//! Environments list: globals, environments, active marker, row actions.

use super::SectionHeader;
use super::menu::{ContextMenu, MenuAction, RenameInput};
use crate::commands::{environments as cmd, files};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::Environment;

#[component]
pub fn EnvironmentsView() -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let envs = state.environments.read().clone();
    let globals = state.globals.read().clone();
    let (svc_new, svc_import) = (services.clone(), services.clone());

    rsx! {
        SectionHeader { title: "Environments",
            Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost, class: "size-7", aria_label: "Import environment", title: "Import Postman environment",
                onclick: move |_| files::import_environment(state, svc_import.clone()), "⇣" }
            Button { size: ButtonSize::IconSm, variant: ButtonVariant::Ghost, class: "size-7", aria_label: "New environment", title: "New environment",
                onclick: move |_| cmd::create(state, svc_new.clone()), "+" }
        }
        div { class: "min-h-0 flex-1 overflow-y-auto px-2 pb-2",
            EnvRow { env: globals }
            div { class: "my-2 h-px bg-border" }
            if envs.is_empty() {
                p { class: "px-2 py-4 text-center text-xs text-muted-foreground", "No environments yet. Create one to switch between servers and credentials." }
            }
            for env in envs {
                EnvRow { key: "{env.id}", env }
            }
        }
    }
}

#[component]
fn EnvRow(env: Environment) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut menu = use_signal(|| None::<(f64, f64)>);
    let id = env.id;
    let globals = id.is_globals();
    let active = *state.active_env.read() == Some(id);
    let renaming = *state.renaming.read() == Some(id.0);
    let count = env
        .variables
        .iter()
        .filter(|v| !v.key.trim().is_empty())
        .count();
    let bg = if active {
        "bg-accent"
    } else {
        "hover:bg-accent/60"
    };
    let actions = if globals {
        vec![MenuAction::Open]
    } else {
        vec![
            MenuAction::Open,
            MenuAction::Rename,
            MenuAction::Duplicate,
            MenuAction::Delete,
        ]
    };
    let name = env.name.clone();
    let svc = services.clone();
    let pick = move |a: MenuAction| match a {
        MenuAction::Rename => state.renaming.set(Some(id.0)),
        MenuAction::Duplicate => cmd::duplicate(state, svc.clone(), id),
        MenuAction::Delete => state
            .dialog
            .set(Some(Dialog::DeleteEnvironment(id, name.clone()))),
        _ => state.dialog.set(Some(Dialog::EditEnvironment(id))),
    };

    rsx! {
        div {
            class: "group flex h-8 cursor-default items-center gap-2 rounded-md px-2 text-sm {bg}",
            onclick: move |_| state.dialog.set(Some(Dialog::EditEnvironment(id))),
            oncontextmenu: move |e: MouseEvent| { e.prevent_default(); let p = e.client_coordinates(); menu.set(Some((p.x, p.y))); },
            if globals {
                span { class: "w-3 shrink-0 text-center text-[10px] text-muted-foreground", "◎" }
            } else {
                button {
                    class: "w-3 shrink-0 text-center text-[10px] text-muted-foreground hover:text-foreground",
                    title: if active { "Active environment (click to deactivate)" } else { "Set as active environment" },
                    "aria-pressed": active,
                    onclick: move |e: MouseEvent| {
                        e.stop_propagation();
                        cmd::set_active(state, if active { None } else { Some(id) });
                    },
                    if active { "●" } else { "○" }
                }
            }
            if renaming {
                RenameInput { id: id.0, value: env.name.clone(), on_commit: move |n: String| cmd::rename(state, services.clone(), id, n) }
            } else {
                span { class: "min-w-0 flex-1 truncate", "{env.name}" }
                span { class: "text-[10px] text-muted-foreground", "{count}" }
                button {
                    class: "invisible shrink-0 rounded px-1 text-muted-foreground hover:bg-accent hover:text-foreground group-hover:visible",
                    "aria-label": "More actions",
                    onclick: move |e: MouseEvent| { e.stop_propagation(); let p = e.client_coordinates(); menu.set(Some((p.x, p.y))); },
                    "⋯"
                }
            }
        }
        if let Some(pos) = *menu.read() {
            ContextMenu { pos, actions, on_pick: pick, on_close: move |_| menu.set(None) }
        }
    }
}

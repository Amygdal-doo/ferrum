//! In-app modal dialogs (never browser `alert`/`confirm`).

mod code;
mod collection_settings;
mod environment;
mod import;
mod save_request;

use crate::commands::{collections, environments, history};
use crate::components::rust_ui::button::{Button, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;

#[component]
pub fn Modal(
    title: String,
    on_close: EventHandler<()>,
    #[props(default = "max-w-xl".to_owned())] width: String,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-4",
            onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { on_close.call(()) },
            div {
                class: "flex max-h-full w-full {width} flex-col rounded-lg border bg-background p-5 shadow-xl",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "{title}",
                h2 { class: "mb-4 shrink-0 text-base font-semibold", "{title}" }
                {children}
            }
        }
    }
}

/// Footer with Cancel and a primary action.
#[component]
pub fn Actions(
    confirm: String,
    on_confirm: EventHandler<()>,
    #[props(default)] destructive: bool,
    #[props(default)] disabled: bool,
) -> Element {
    let mut state = use_context::<AppState>();
    let variant = if destructive {
        ButtonVariant::Destructive
    } else {
        ButtonVariant::Default
    };
    rsx! {
        div { class: "mt-5 flex shrink-0 justify-end gap-2",
            Button { variant: ButtonVariant::Ghost, onclick: move |_| state.dialog.set(None), "Cancel" }
            Button { variant, disabled, onclick: move |_| on_confirm.call(()), "{confirm}" }
        }
    }
}

#[component]
pub fn DialogHost() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let dialog = state.dialog.read().clone();
    let close = move |_| state.dialog.set(None);

    match dialog {
        None => rsx! {},
        Some(Dialog::SaveRequest(tab_id)) => rsx! { save_request::SaveRequestDialog { tab_id } },
        Some(Dialog::CollectionSettings(id)) => {
            rsx! { collection_settings::CollectionSettingsDialog { id } }
        }
        Some(Dialog::EditEnvironment(id)) => {
            rsx! { environment::EnvironmentDialog { key: "{id}", id } }
        }
        Some(Dialog::Import) => rsx! { import::ImportDialog {} },
        Some(Dialog::Code(tab_id)) => rsx! { code::CodeDialog { tab_id } },
        Some(Dialog::DeleteItem {
            collection,
            item,
            name,
        }) => {
            let what = if item.is_some() { "item" } else { "collection" };
            rsx! {
                Modal { title: "Delete {what}?", on_close: close,
                    p { class: "text-sm", "“{name}” and everything inside it will be deleted. Open tabs are kept as unsaved requests." }
                    Actions { confirm: "Delete", destructive: true,
                        on_confirm: move |_| { state.dialog.set(None); collections::delete(state, services.clone(), collection, item); } }
                }
            }
        }
        Some(Dialog::DeleteEnvironment(id, name)) => rsx! {
            Modal { title: "Delete environment?", on_close: close,
                p { class: "text-sm", "“{name}” and its variables will be deleted." }
                Actions { confirm: "Delete", destructive: true,
                    on_confirm: move |_| { state.dialog.set(None); environments::delete(state, services.clone(), id); } }
            }
        },
        Some(Dialog::ClearHistory) => rsx! {
            Modal { title: "Clear history?", on_close: close,
                p { class: "text-sm", "All history entries will be deleted. This cannot be undone." }
                Actions { confirm: "Clear history", destructive: true,
                    on_confirm: move |_| { state.dialog.set(None); history::clear(state, services.clone()); } }
            }
        },
    }
}

//! In-app modal dialogs (never browser `alert`/`confirm`).

use crate::commands::{connection, query};
use crate::components::connection::ConnectionForm;
use crate::components::rust_ui::button::{Button, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrum_application::AppServices;

#[component]
pub fn Modal(title: String, on_close: EventHandler<()>, children: Element) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-4",
            onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { on_close.call(()) },
            div {
                class: "w-full max-w-xl rounded-lg border bg-background p-5 shadow-xl",
                role: "dialog",
                "aria-modal": "true",
                "aria-label": "{title}",
                h2 { class: "mb-4 text-base font-semibold", "{title}" }
                {children}
            }
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
        Some(Dialog::ConnectionForm(existing)) => rsx! { ConnectionForm { existing } },
        Some(Dialog::DeleteProfile(profile)) => {
            let id = profile.id;
            rsx! {
                Modal { title: "Delete connection?".to_owned(), on_close: close,
                    p { class: "text-sm", "“{profile.display_name}” and its saved password will be removed. Editor tabs are kept." }
                    div { class: "mt-5 flex justify-end gap-2",
                        Button { variant: ButtonVariant::Ghost, onclick: move |_| state.dialog.set(None), "Cancel" }
                        Button { variant: ButtonVariant::Destructive,
                            onclick: move |_| { state.dialog.set(None); connection::delete(state, services.clone(), id); },
                            "Delete"
                        }
                    }
                }
            }
        }
        Some(Dialog::ConfirmRun {
            tab_id,
            profile,
            sql,
        }) => {
            let preview: String = sql.chars().take(600).collect();
            let (p, s) = (profile.clone(), sql.clone());
            rsx! {
                Modal { title: "Run on production?".to_owned(), on_close: close,
                    p { class: "text-sm",
                        "This statement may modify data or schema on "
                        strong { "{profile.display_name}" }
                        " ({profile.target_label()}), tagged "
                        strong { class: "text-destructive", "Production" }
                        "."
                    }
                    pre { class: "mt-3 max-h-48 overflow-auto rounded-md bg-muted p-3 font-mono text-xs select-text whitespace-pre-wrap", "{preview}" }
                    div { class: "mt-5 flex justify-end gap-2",
                        Button { variant: ButtonVariant::Ghost, onclick: move |_| state.dialog.set(None), "Cancel" }
                        Button { variant: ButtonVariant::Destructive,
                            onclick: move |_| {
                                state.dialog.set(None);
                                query::start_run(state, services.clone(), tab_id, p.clone(), s.clone());
                            },
                            "Run on production"
                        }
                    }
                }
            }
        }
    }
}

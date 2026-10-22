//! Response viewer: empty / loading / error states and the response itself.

mod body_view;
mod tables;

use crate::commands::{files, send};
use crate::components::rust_ui::badge::{Badge, BadgeVariant};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, TabResponse};
use body_view::BodyView;
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::HttpResponse;
use ferrumapi_domain::text::{human_duration_ms, human_size};
use tables::{CookiesTable, HeadersTable};

#[component]
pub fn ResponsePanel() -> Element {
    let state = use_context::<AppState>();
    let tab_id = *state.active_tab.read();
    let response = tab_id.and_then(|id| state.responses.read().get(&id).cloned());

    let content = match (tab_id, response) {
        (_, None) | (None, _) => rsx! {
            Centered {
                p { class: "text-sm", "No response yet" }
                p { class: "text-xs", "Enter a URL and press Send (⌘/Ctrl+Enter)." }
            }
        },
        (Some(id), Some(TabResponse::Loading { .. })) => rsx! {
            Centered {
                div { class: "size-6 animate-spin rounded-full border-2 border-muted-foreground border-t-transparent", role: "progressbar", "aria-label": "Sending request" }
                p { class: "text-sm", "Sending request…" }
                Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: move |_| send::cancel(state, id), "Cancel" }
            }
        },
        (_, Some(TabResponse::Failed(msg))) => rsx! {
            Centered {
                p { class: "text-sm font-medium text-destructive", "Could not get a response" }
                p { class: "max-w-xl text-center text-xs select-text", "{msg}" }
            }
        },
        (_, Some(TabResponse::Cancelled)) => rsx! {
            Centered { p { class: "text-sm", "Request cancelled." } }
        },
        (_, Some(TabResponse::Done(resp))) => rsx! { ResponseView { response: *resp } },
    };

    rsx! {
        section { class: "flex h-full min-h-0 flex-col bg-background", "aria-label": "Response",
            {content}
        }
    }
}

#[component]
fn Centered(children: Element) -> Element {
    rsx! {
        div { class: "flex flex-1 flex-col items-center justify-center gap-2 text-muted-foreground", {children} }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum View {
    Body,
    Headers,
    Cookies,
}

fn status_variant(status: u16) -> BadgeVariant {
    match status {
        200..=299 => BadgeVariant::Success,
        300..=399 => BadgeVariant::Info,
        400..=499 => BadgeVariant::Warning,
        _ => BadgeVariant::Destructive,
    }
}

#[component]
fn ResponseView(response: HttpResponse) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut view = use_signal(|| View::Body);
    let current = *view.read();
    let r = response.clone();
    let views = [
        (View::Body, "Body".to_owned()),
        (
            View::Headers,
            format!("Headers ({})", response.headers.len()),
        ),
        (
            View::Cookies,
            format!("Cookies ({})", response.cookies.len()),
        ),
    ];
    let copy_src = response.clone();
    let save_src = response.clone();

    rsx! {
        div { class: "flex h-9 shrink-0 items-center gap-3 border-b px-3 text-xs",
            for (v, label) in views {
                {
                    let cls = if v == current { "border-foreground text-foreground" } else { "border-transparent text-muted-foreground hover:text-foreground" };
                    rsx! {
                        button { key: "{label}", class: "flex h-9 items-center border-b-2 font-medium {cls}", onclick: move |_| view.set(v), "{label}" }
                    }
                }
            }
            div { class: "flex-1" }
            Badge { variant: status_variant(r.status), "{r.status} {r.status_text}" }
            span { class: "text-muted-foreground", title: "Time", "{human_duration_ms(r.time_ms)}" }
            span { class: "text-muted-foreground", title: "Size", "{human_size(r.size_bytes)}" }
            Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-7 px-2 text-xs",
                title: "Copy the response body",
                onclick: move |_| files::copy_text(state, copy_src.body_text()),
                "Copy"
            }
            Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-7 px-2 text-xs",
                title: "Save the response body to a file",
                onclick: move |_| files::save_response(state, services.clone(), save_src.body.clone(), files::response_file_name(&save_src)),
                "Save…"
            }
        }
        div { class: "min-h-0 flex-1 overflow-hidden",
            match current {
                View::Body => rsx! { BodyView { response } },
                View::Headers => rsx! { HeadersTable { headers: response.headers } },
                View::Cookies => rsx! { CookiesTable { cookies: response.cookies } },
            }
        }
    }
}

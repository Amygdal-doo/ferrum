//! Response body: Pretty | Raw | Preview.

use base64::Engine;
use dioxus::prelude::*;
use ferrumapi_domain::HttpResponse;
use ferrumapi_domain::text::pretty_json;

/// Large bodies are cut for display (Copy / Save still use the full body).
const MAX_RENDER_CHARS: usize = 400_000;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Pretty,
    Raw,
    Preview,
}

fn clip(text: String) -> (String, bool) {
    if text.len() <= MAX_RENDER_CHARS {
        return (text, false);
    }
    let mut end = MAX_RENDER_CHARS;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    (text[..end].to_owned(), true)
}

fn pretty(resp: &HttpResponse) -> String {
    let text = resp.body_text();
    let looks_json = resp.is_json() || text.trim_start().starts_with(['{', '[']);
    if looks_json {
        pretty_json(&text).unwrap_or(text)
    } else {
        text
    }
}

#[component]
pub fn BodyView(response: HttpResponse) -> Element {
    let mut mode = use_signal(|| Mode::Pretty);
    let current = *mode.read();
    let modes = [
        (Mode::Pretty, "Pretty"),
        (Mode::Raw, "Raw"),
        (Mode::Preview, "Preview"),
    ];
    let content_type = response.content_type().unwrap_or_default().to_owned();
    let is_image = content_type.starts_with("image/");

    let body = match current {
        Mode::Pretty | Mode::Raw if is_image => rsx! {
            p { class: "p-3 text-xs text-muted-foreground", "Binary image body ({content_type}). Use Preview to view it." }
        },
        Mode::Pretty | Mode::Raw => {
            let text = if current == Mode::Pretty {
                pretty(&response)
            } else {
                response.body_text()
            };
            let (text, clipped) = clip(text);
            rsx! {
                if clipped {
                    p { class: "border-b bg-warning-light px-3 py-1 text-xs text-warning-dark", "Large body: showing the first 400 KB. Use Save to get the full body." }
                }
                pre { class: "min-h-0 flex-1 overflow-auto p-3 font-mono text-xs leading-5 select-text whitespace-pre-wrap break-all", "{text}" }
            }
        }
        Mode::Preview if is_image => {
            let data = base64::engine::general_purpose::STANDARD.encode(&response.body);
            rsx! {
                div { class: "flex min-h-0 flex-1 items-start justify-center overflow-auto p-3",
                    img { class: "max-w-full", alt: "Response image", src: "data:{content_type};base64,{data}" }
                }
            }
        }
        Mode::Preview if response.is_html() => rsx! {
            // Empty sandbox: no scripts, forms, popups or same-origin access.
            iframe { class: "min-h-0 w-full flex-1 border-0 bg-white", "sandbox": "", title: "HTML preview", srcdoc: "{response.body_text()}" }
        },
        Mode::Preview => rsx! {
            p { class: "p-3 text-xs text-muted-foreground", "Preview is available for HTML and image responses." }
        },
    };

    rsx! {
        div { class: "flex h-full min-h-0 flex-col",
            div { class: "flex h-8 shrink-0 items-center gap-1 px-3",
                for (m, label) in modes {
                    {
                        let cls = if m == current { "bg-accent text-foreground" } else { "text-muted-foreground hover:text-foreground" };
                        rsx! { button { key: "{label}", class: "rounded px-2 py-0.5 text-xs {cls}", onclick: move |_| mode.set(m), "{label}" } }
                    }
                }
            }
            {body}
        }
    }
}

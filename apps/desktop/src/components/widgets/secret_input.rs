//! Masked input with a show / hide toggle.

use super::FIELD;
use dioxus::prelude::*;

#[component]
pub fn SecretInput(
    value: String,
    on_input: EventHandler<String>,
    #[props(default)] placeholder: String,
    #[props(default)] label: String,
) -> Element {
    let mut shown = use_signal(|| false);
    let kind = if *shown.read() { "text" } else { "password" };
    let toggle = if *shown.read() { "Hide" } else { "Show" };
    rsx! {
        div { class: "flex items-center gap-1",
            input {
                class: "{FIELD} font-mono",
                r#type: kind,
                placeholder,
                "aria-label": "{label}",
                spellcheck: "false",
                autocomplete: "off",
                initial_value: "{value}",
                oninput: move |e| on_input.call(e.value()),
            }
            button {
                class: "h-8 shrink-0 rounded-md px-2 text-xs text-muted-foreground hover:bg-accent hover:text-foreground",
                "aria-pressed": *shown.read(),
                onclick: move |_| { let v = !*shown.peek(); shown.set(v); },
                "{toggle}"
            }
        }
    }
}

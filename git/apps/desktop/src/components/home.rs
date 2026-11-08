//! Start screen: open / clone / init, and recent repositories with their profiles.

use crate::commands::workspace;
use crate::components::now_secs;
use crate::components::rust_ui::button::{Button, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::text::relative_time;

#[component]
pub fn Home() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let mut filter = use_signal(String::new);
    let needle = filter.read().to_lowercase();
    let repos: Vec<_> = state
        .repos
        .read()
        .iter()
        .filter(|r| {
            needle.is_empty()
                || r.name.to_lowercase().contains(&needle)
                || r.path.to_lowercase().contains(&needle)
        })
        .cloned()
        .collect();
    let none = state.repos.read().is_empty();
    let no_profiles = state.profiles.read().is_empty();
    let (s_open, s_init) = (services.clone(), services.clone());
    let now = now_secs();

    rsx! {
        div { class: "flex min-h-0 flex-1 justify-center overflow-y-auto p-10",
            div { class: "flex w-full max-w-3xl flex-col gap-8",
                div {
                    h1 { class: "text-2xl font-semibold tracking-tight", "Repositories" }
                    p { class: "mt-1 text-sm text-muted-foreground",
                        "Each repository can use its own profile — a commit identity and an SSH key — for commit, fetch, pull and push." }
                }
                div { class: "grid grid-cols-3 gap-3",
                    HomeAction { title: "Open", hint: "An existing local repository", onclick: move |_| workspace::pick_and_open(state, s_open.clone()) }
                    HomeAction { title: "Clone", hint: "From a URL, with a chosen SSH profile", onclick: move |_| state.dialog.set(Some(Dialog::Clone)) }
                    HomeAction { title: "Init", hint: "Start a new repository in a folder", onclick: move |_| workspace::pick_and_init(state, s_init.clone()) }
                }
                if no_profiles {
                    div { class: "flex items-center gap-3 rounded-lg border border-dashed p-4 text-sm",
                        div { class: "flex-1",
                            p { class: "font-medium", "Set up a profile" }
                            p { class: "text-muted-foreground", "Add your work and personal identities with their SSH keys, then pick one per repository." }
                        }
                        Button { variant: ButtonVariant::Outline, onclick: move |_| state.dialog.set(Some(Dialog::Profiles(None))), "Add profile" }
                    }
                }
                div { class: "flex flex-col gap-2",
                    div { class: "flex items-center gap-2",
                        h2 { class: "flex-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Recent" }
                        if !none {
                            input { class: "h-7 w-56 rounded-md border bg-background px-2 text-xs select-text", placeholder: "Filter",
                                "aria-label": "Filter repositories", value: "{filter}", oninput: move |e| filter.set(e.value()) }
                        }
                    }
                    if none {
                        p { class: "rounded-lg border p-6 text-center text-sm text-muted-foreground", "No repositories yet. Open, clone or init one above." }
                    }
                    div { class: "divide-y rounded-lg border",
                        for r in repos {
                            {
                                let id = r.id;
                                let profile = state.profile_of(&r).map_or_else(|| "System default".to_owned(), |p| p.name);
                                let when = if r.last_opened_ms > 0 { relative_time(r.last_opened_ms / 1000, now) } else { String::new() };
                                let (s1, s2) = (services.clone(), services.clone());
                                rsx! {
                                    div { key: "{id}", class: "group flex cursor-pointer items-center gap-3 px-4 py-2.5 hover:bg-accent/60",
                                        onclick: move |_| workspace::activate(state, s1.clone(), Some(id)),
                                        div { class: "min-w-0 flex-1",
                                            p { class: "truncate text-sm font-medium", "{r.name}" }
                                            p { class: "truncate text-xs text-muted-foreground", "{r.path}" }
                                        }
                                        span { class: "rounded-md bg-muted px-2 py-0.5 text-[11px] text-muted-foreground", title: "Profile", "{profile}" }
                                        span { class: "w-20 text-right text-[11px] text-muted-foreground", "{when}" }
                                        button { class: "rounded px-1.5 text-xs text-muted-foreground opacity-0 hover:bg-background group-hover:opacity-100",
                                            title: "Remove from list (files are kept)",
                                            onclick: move |e| { e.stop_propagation(); workspace::forget(state, s2.clone(), id); }, "×" }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

#[component]
fn HomeAction(title: String, hint: String, onclick: EventHandler<()>) -> Element {
    rsx! {
        button { class: "flex flex-col items-start gap-1 rounded-lg border p-4 text-left hover:bg-accent/60",
            onclick: move |_| onclick.call(()),
            span { class: "text-sm font-semibold", "{title}" }
            span { class: "text-xs text-muted-foreground", "{hint}" }
        }
    }
}

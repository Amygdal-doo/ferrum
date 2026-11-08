//! Per-repository settings: which profile (identity + SSH key) it uses, remotes.

use super::{FIELD, Modal};
use crate::commands::{profiles, workspace};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::PullMode;
use ferrumgit_domain::{ProfileId, RepoId, RepoOp};

#[component]
pub fn RepoSettingsDialog(id: RepoId) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let Some(entry) = state.entry(id) else {
        return rsx! {};
    };
    let current = entry.profile;
    let list = state.profiles.read().clone();
    let remotes = state
        .view(id)
        .data
        .map(|d| d.snapshot.remotes.clone())
        .unwrap_or_default();
    let ssh_remote = remotes.iter().any(ferrumgit_domain::Remote::is_ssh);
    let (s_none, s_forget) = (services.clone(), services.clone());
    let path = entry.path.clone();
    let (pull_mode, prune, show_ws) = {
        let s = state.settings.read();
        (s.pull_mode, s.prune_on_fetch, !s.hide_invisibles)
    };
    let (s_prefs, s_prefs2, s_prefs3) = (services.clone(), services.clone(), services.clone());
    let pick = move |p: Option<ProfileId>| profiles::assign(state, services.clone(), id, p);

    rsx! {
        Modal { title: "Repository settings — {entry.name}", width: "max-w-2xl", on_close: move |_| state.dialog.set(None),
            div { class: "flex min-h-0 flex-col gap-5 overflow-y-auto text-sm",
                section { class: "flex flex-col gap-2",
                    div { class: "flex items-center",
                        h3 { class: "flex-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Profile for this repository" }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-7 text-xs",
                            onclick: move |_| state.dialog.set(Some(Dialog::Profiles(current))), "Manage profiles…" }
                    }
                    p { class: "text-xs text-muted-foreground",
                        "Commits use the profile's name and email; fetch, pull, push and clone use its SSH key (only that key is offered to the server)." }
                    div { class: "flex flex-col divide-y rounded-md border",
                        ProfileChoice { title: "System default", hint: "Global git identity · ssh-agent / ~/.ssh/config", selected: current.is_none(),
                            onpick: move |_| profiles::assign(state, s_none.clone(), id, None) }
                        for p in list {
                            {
                                let pid = p.id;
                                let who = p.identity().map_or_else(|| "git config identity".to_owned(), |(n, e)| format!("{n} <{e}>"));
                                let pick = pick.clone();
                                rsx! {
                                    ProfileChoice { key: "{pid}", title: p.name.clone(), hint: format!("{who} · 🔑 {}", p.ssh_label()), selected: current == Some(pid),
                                        onpick: move |_| pick(Some(pid)) }
                                }
                            }
                        }
                    }
                    if !ssh_remote && !remotes.is_empty() {
                        p { class: "text-xs text-warning-dark", "This repository's remotes use HTTPS, so the SSH key is not used; git's credential helper handles authentication." }
                    }
                }
                section { class: "flex flex-col gap-2",
                    div { class: "flex items-center",
                        h3 { class: "flex-1 text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Remotes" }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "h-7 text-xs",
                            onclick: move |_| state.dialog.set(Some(Dialog::AddRemote)), "Add remote…" }
                    }
                    if remotes.is_empty() { p { class: "text-xs text-muted-foreground", "No remotes." } }
                    for r in remotes {
                        div { key: "{r.name}", class: "flex items-center gap-2 rounded-md border px-3 py-2",
                            span { class: "w-24 shrink-0 font-medium", "{r.name}" }
                            span { class: "min-w-0 flex-1 truncate font-mono text-xs text-muted-foreground select-text", title: "{r.fetch_url}", "{r.fetch_url}" }
                            span { class: "rounded bg-muted px-1.5 text-[10px] text-muted-foreground", if r.is_ssh() { "SSH" } else { "HTTPS/local" } }
                            button { class: "rounded px-1.5 text-xs text-destructive hover:bg-accent", title: "Remove remote",
                                onclick: move |_| state.dialog.set(Some(Dialog::Confirm { title: format!("Remove remote {}?", r.name), body: "Its remote-tracking branches are removed too.".into(), confirm: "Remove".into(), destructive: true, op: RepoOp::RemoveRemote(r.name.clone()) })),
                                "Remove" }
                        }
                    }
                }
                section { class: "flex flex-col gap-2",
                    h3 { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Preferences (all repositories)" }
                    label { class: "flex items-center gap-3 text-sm",
                        span { class: "w-28 text-muted-foreground", "Pull" }
                        select { class: "h-8 rounded-md border bg-background px-2 text-sm",
                            onchange: move |e| {
                                state.settings.write().pull_mode = PullMode::ALL.into_iter().find(|m| m.label() == e.value()).unwrap_or_default();
                                workspace::save_settings(state, s_prefs.clone());
                            },
                            for m in PullMode::ALL {
                                option { key: "{m.label()}", value: "{m.label()}", selected: m == pull_mode, "{m.label()}" }
                            }
                        }
                    }
                    label { class: "flex items-center gap-3 text-sm",
                        input { r#type: "checkbox", checked: prune, onchange: move |e| { state.settings.write().prune_on_fetch = e.checked(); workspace::save_settings(state, s_prefs2.clone()); } }
                        "Prune deleted remote branches when fetching"
                    }
                    label { class: "flex items-center gap-3 text-sm",
                        input { r#type: "checkbox", checked: show_ws, onchange: move |e| { state.settings.write().hide_invisibles = !e.checked(); workspace::save_settings(state, s_prefs3.clone()); } }
                        "Show whitespace (· and ¬) in diffs"
                    }
                }
                section { class: "flex flex-col gap-2",
                    h3 { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Location" }
                    input { class: "{FIELD} font-mono text-xs", readonly: true, value: "{entry.path}" }
                    div { class: "flex gap-2",
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, onclick: move |_| workspace::reveal(&path), "Open folder" }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "text-destructive", title: "Removes it from FerrumGIT; files are kept",
                            onclick: move |_| { state.dialog.set(None); workspace::forget(state, s_forget.clone(), id); }, "Remove from FerrumGIT" }
                    }
                }
            }
            div { class: "mt-5 flex justify-end",
                Button { onclick: move |_| state.dialog.set(None), "Done" }
            }
        }
    }
}

#[component]
fn ProfileChoice(title: String, hint: String, selected: bool, onpick: EventHandler<()>) -> Element {
    rsx! {
        label { class: "flex cursor-pointer items-center gap-3 px-3 py-2 hover:bg-accent/50",
            input { r#type: "radio", name: "repo-profile", checked: selected, onchange: move |_| onpick.call(()) }
            div { class: "min-w-0",
                p { class: "font-medium", "{title}" }
                p { class: "truncate text-xs text-muted-foreground", "{hint}" }
            }
        }
    }
}

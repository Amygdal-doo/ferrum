//! In-app modal dialogs (never browser `alert`/`confirm`).

mod clone;
mod pickers;
mod profiles;
mod repo_settings;

use crate::commands::repo;
use crate::components::rust_ui::button::{Button, ButtonVariant};
use crate::state::{AppState, Dialog, PromptKind};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{RepoOp, ResetMode};

/// Standard bordered field.
pub const FIELD: &str = "h-8 w-full rounded-md border bg-background px-2 text-sm outline-none focus:border-ring select-text";

#[component]
pub fn Modal(
    title: String,
    on_close: EventHandler<()>,
    #[props(default = "max-w-lg".to_owned())] width: String,
    children: Element,
) -> Element {
    rsx! {
        div {
            class: "fixed inset-0 z-40 flex items-center justify-center bg-black/40 p-4",
            onkeydown: move |e: KeyboardEvent| if e.key() == Key::Escape { on_close.call(()) },
            div {
                class: "flex max-h-full w-full {width} flex-col rounded-lg border bg-background p-5 shadow-xl",
                role: "dialog", "aria-modal": "true", "aria-label": "{title}",
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
    let mut run = move |op: RepoOp| {
        state.dialog.set(None);
        repo::active_op(state, services.clone(), op);
    };

    match dialog {
        None => rsx! {},
        Some(Dialog::Confirm {
            title,
            body,
            confirm,
            destructive,
            op,
        }) => rsx! {
            Modal { title, on_close: close,
                p { class: "whitespace-pre-wrap text-sm", "{body}" }
                Actions { confirm, destructive, on_confirm: move |_| run(op.clone()) }
            }
        },
        Some(Dialog::Prompt { kind, initial }) => {
            rsx! { PromptDialog { kind, initial, on_run: run } }
        }
        Some(Dialog::Reset { target, label }) => {
            rsx! { ResetDialog { target, label, on_run: run } }
        }
        Some(Dialog::AddRemote) => rsx! { AddRemoteDialog { on_run: run } },
        Some(Dialog::Clone) => rsx! { clone::CloneDialog {} },
        Some(Dialog::QuickOpen) => rsx! { pickers::QuickOpen {} },
        Some(Dialog::PickBranch { rebase }) => rsx! { pickers::PickBranch { rebase } },
        Some(Dialog::RepoSettings(id)) => rsx! { repo_settings::RepoSettingsDialog { id } },
        Some(Dialog::Profiles(selected)) => rsx! { profiles::ProfilesDialog { selected } },
    }
}

#[component]
fn PromptDialog(kind: PromptKind, initial: String, on_run: EventHandler<RepoOp>) -> Element {
    let mut state = use_context::<AppState>();
    let (title, field, message_label, check) = kind.labels();
    let mut text = use_signal(|| initial.clone());
    let mut message = use_signal(String::new);
    let mut checked = use_signal(|| check.is_some_and(|c| c.1));
    let subtitle = match &kind {
        PromptKind::CreateBranch { start: Some(s) } => format!("Starting at {}", short(s)),
        PromptKind::CreateBranch { start: None } => "Starting at HEAD".to_owned(),
        PromptKind::RenameBranch { old } => format!("Renaming “{old}”"),
        PromptKind::CreateTag { target } => format!("At {}", short(target)),
        PromptKind::CheckoutRemoteAs { remote_branch } => {
            format!("A local branch with that name exists; track {remote_branch} as:")
        }
        PromptKind::StashSave => String::new(),
    };
    let k = kind.clone();
    let submit = move || {
        if let Some(op) = k.build(&text.peek(), &message.peek(), *checked.peek()) {
            on_run.call(op);
        }
    };
    let submit2 = submit.clone();
    let valid = kind == PromptKind::StashSave || !text.read().trim().is_empty();
    rsx! {
        Modal { title: title.to_owned(), on_close: move |_| state.dialog.set(None),
            div { class: "flex flex-col gap-3 text-sm",
                if !subtitle.is_empty() { p { class: "text-xs text-muted-foreground", "{subtitle}" } }
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "{field}" }
                    input { class: FIELD, initial_value: "{initial}",
                        onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; },
                        oninput: move |e| text.set(e.value()),
                        onkeydown: move |e: KeyboardEvent| if e.key() == Key::Enter { submit() } }
                }
                if let Some(label) = message_label {
                    label { class: "flex flex-col gap-1",
                        span { class: "text-xs text-muted-foreground", "{label}" }
                        textarea { class: "h-20 w-full resize-none rounded-md border bg-background px-2 py-1.5 text-sm outline-none focus:border-ring select-text",
                            oninput: move |e| message.set(e.value()) }
                    }
                }
                if let Some((label, _)) = check {
                    label { class: "flex items-center gap-2 text-xs",
                        input { r#type: "checkbox", checked: *checked.read(), onchange: move |e| checked.set(e.checked()) }
                        "{label}"
                    }
                }
            }
            Actions { confirm: title.to_owned(), disabled: !valid, on_confirm: move |_| submit2() }
        }
    }
}

fn short(rev: &str) -> &str {
    if rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()) {
        &rev[..7]
    } else {
        rev
    }
}

#[component]
fn ResetDialog(target: String, label: String, on_run: EventHandler<RepoOp>) -> Element {
    let mut state = use_context::<AppState>();
    let mut mode = use_signal(|| ResetMode::Mixed);
    let modes = [
        (ResetMode::Soft, "Soft", "Keep all changes staged."),
        (
            ResetMode::Mixed,
            "Mixed",
            "Keep changes in the working copy, unstaged.",
        ),
        (
            ResetMode::Hard,
            "Hard",
            "Discard all changes. This cannot be undone.",
        ),
    ];
    let current = *mode.read();
    rsx! {
        Modal { title: "Reset current branch to {label}?", on_close: move |_| state.dialog.set(None),
            div { class: "flex flex-col gap-2 text-sm",
                for (m, name, hint) in modes {
                    label { key: "{name}", class: "flex cursor-pointer items-start gap-2 rounded-md border p-2 hover:bg-accent/50",
                        input { r#type: "radio", name: "reset-mode", class: "mt-1", checked: current == m, onchange: move |_| mode.set(m) }
                        div { p { class: "font-medium", "{name}" } p { class: "text-xs text-muted-foreground", "{hint}" } }
                    }
                }
            }
            Actions { confirm: "Reset", destructive: current == ResetMode::Hard,
                on_confirm: move |_| on_run.call(RepoOp::Reset { target: target.clone(), mode: *mode.peek() }) }
        }
    }
}

#[component]
fn AddRemoteDialog(on_run: EventHandler<RepoOp>) -> Element {
    let mut state = use_context::<AppState>();
    let has_origin = state
        .active_entry()
        .and_then(|e| state.view(e.id).data)
        .is_some_and(|d| d.snapshot.remotes.iter().any(|r| r.name == "origin"));
    let initial = if has_origin { "upstream" } else { "origin" };
    let mut name = use_signal(|| initial.to_owned());
    let mut url = use_signal(String::new);
    let valid = !name.read().trim().is_empty() && !url.read().trim().is_empty();
    rsx! {
        Modal { title: "Add remote", on_close: move |_| state.dialog.set(None),
            div { class: "flex flex-col gap-3 text-sm",
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Name" }
                    input { class: FIELD, initial_value: "{initial}", oninput: move |e| name.set(e.value()) }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "URL" }
                    input { class: FIELD, placeholder: "git@github.com:owner/repo.git",
                        onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; },
                        oninput: move |e| url.set(e.value()) }
                }
                p { class: "text-xs text-muted-foreground", "SSH URLs authenticate with this repository's profile key." }
            }
            Actions { confirm: "Add remote", disabled: !valid,
                on_confirm: move |_| on_run.call(RepoOp::AddRemote { name: name.peek().trim().to_owned(), url: url.peek().trim().to_owned() }) }
        }
    }
}

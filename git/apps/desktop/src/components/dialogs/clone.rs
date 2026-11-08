//! Clone a repository with a chosen profile (its SSH key is used for the clone
//! and the repository keeps that profile afterwards).

use super::{FIELD, Modal};
use crate::commands::{run, workspace};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumgit_application::{AppServices, CancellationToken};
use ferrumgit_domain::ProfileId;
use uuid::Uuid;

/// `git@host:o/repo.git` → `repo`.
fn repo_name(url: &str) -> String {
    let u = url.trim().trim_end_matches('/');
    let last = u.rsplit(['/', ':']).next().unwrap_or(u);
    last.strip_suffix(".git").unwrap_or(last).to_owned()
}

#[component]
pub fn CloneDialog() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let home = std::env::var("HOME").unwrap_or_default();
    let mut url = use_signal(String::new);
    let mut parent = use_signal(|| home.clone());
    let mut parent_rev = use_signal(|| 0u64);
    let mut name = use_signal(String::new);
    let mut name_edited = use_signal(|| false);
    let mut name_rev = use_signal(|| 0u64);
    let profiles = state.profiles.read().clone();
    let mut profile = use_signal(|| profiles.first().map(|p| p.id));
    let mut running = use_signal(|| None::<CancellationToken>);
    let dest = format!(
        "{}/{}",
        parent.read().trim_end_matches('/'),
        name.read().trim()
    );
    let valid =
        !url.read().trim().is_empty() && !name.read().trim().is_empty() && running.read().is_none();
    let dest2 = dest.clone();

    let start = move |_| {
        let token = CancellationToken::new();
        running.set(Some(token.clone()));
        let p = (*profile.peek())
            .and_then(|id| state.profiles.peek().iter().find(|p| p.id == id).cloned());
        let (s, u, d) = (services.clone(), url.peek().clone(), dest2.clone());
        spawn(async move {
            let s2 = s.clone();
            let result = run(
                &s,
                async move { s2.clone_repo(&u, &d, p, Some(token)).await },
            )
            .await;
            running.set(None);
            match result {
                Ok(entry) => {
                    state.dialog.set(None);
                    state.notify(format!("Cloned {}", entry.name), false);
                    workspace::upsert_and_open(state, s, entry);
                }
                Err(e) => state.notify(e, true),
            }
        });
    };

    rsx! {
        Modal { title: "Clone repository", width: "max-w-xl", on_close: move |_| if running.peek().is_none() { state.dialog.set(None) },
            div { class: "flex flex-col gap-3 text-sm",
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Repository URL" }
                    input { class: "{FIELD} font-mono text-xs", placeholder: "git@github.com:owner/repo.git",
                        onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; },
                        oninput: move |e| {
                            url.set(e.value());
                            if !*name_edited.peek() { name.set(repo_name(&e.value())); name_rev += 1; }
                        } }
                }
                div { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Clone into" }
                    div { class: "flex gap-2",
                        for r in [*parent_rev.read()] {
                            input { key: "{r}", class: "{FIELD} font-mono text-xs", initial_value: "{parent.peek()}", oninput: move |e| parent.set(e.value()) }
                        }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline,
                            onclick: move |_| { spawn(async move {
                                if let Some(d) = rfd::AsyncFileDialog::new().set_title("Clone into folder").pick_folder().await {
                                    parent.set(d.path().display().to_string());
                                    parent_rev += 1;
                                }
                            }); },
                            "Browse…" }
                    }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Folder name" }
                    for r in [*name_rev.read()] {
                        input { key: "{r}", class: FIELD, initial_value: "{name.peek()}", oninput: move |e| { name_edited.set(true); name.set(e.value()); } }
                    }
                    span { class: "truncate font-mono text-[11px] text-muted-foreground", "{dest}" }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Profile (SSH key + identity for this repository)" }
                    select { class: FIELD,
                        onchange: move |e| profile.set(Uuid::parse_str(&e.value()).ok().map(ProfileId)),
                        option { value: "", selected: profile.read().is_none(), "System default (ssh-agent / ~/.ssh/config)" }
                        for p in profiles {
                            option { key: "{p.id}", value: "{p.id}", selected: *profile.read() == Some(p.id), "{p.name} — {p.ssh_label()}" }
                        }
                    }
                }
            }
            div { class: "mt-5 flex items-center justify-end gap-2",
                if let Some(token) = running.read().clone() {
                    span { class: "flex-1 text-xs text-muted-foreground", "Cloning…" }
                    Button { variant: ButtonVariant::Ghost, onclick: move |_| token.cancel(), "Cancel clone" }
                } else {
                    Button { variant: ButtonVariant::Ghost, onclick: move |_| state.dialog.set(None), "Cancel" }
                }
                Button { disabled: !valid, onclick: start, "Clone" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::repo_name;

    #[test]
    fn derives_folder_name() {
        assert_eq!(repo_name("git@github.com:o/ferrum.git"), "ferrum");
        assert_eq!(repo_name("https://gitlab.com/g/sub/app/"), "app");
        assert_eq!(repo_name("ssh://git@host:2222/x/y.git"), "y");
    }
}

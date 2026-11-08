//! Profiles manager: identity + SSH key, passphrase in the OS keychain,
//! key generation and "Test connection".

use super::{FIELD, Modal};
use crate::commands::{self, profiles, run};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumgit_application::{AppServices, PassphraseChange};
use ferrumgit_domain::{Profile, ProfileId, Secret, SshAuth};

#[component]
pub fn ProfilesDialog(selected: Option<ProfileId>) -> Element {
    let mut state = use_context::<AppState>();
    let list = state.profiles.read().clone();
    let mut current = use_signal(move || selected.or_else(|| list.first().map(|p| p.id)));
    let mut draft_new = use_signal(|| None::<Profile>);
    let list = state.profiles.read().clone();
    let editing: Option<Profile> = draft_new
        .read()
        .clone()
        .or_else(|| (*current.read()).and_then(|id| list.iter().find(|p| p.id == id).cloned()));

    rsx! {
        Modal { title: "Profiles", width: "max-w-4xl", on_close: move |_| state.dialog.set(None),
            p { class: "-mt-2 mb-4 text-xs text-muted-foreground",
                "A profile is a commit identity plus the SSH key used for network operations. Assign one per repository in its settings (toolbar → Profile)." }
            div { class: "flex min-h-0 flex-1 gap-4",
                div { class: "flex w-56 shrink-0 flex-col gap-1",
                    for p in list.clone() {
                        {
                            let pid = p.id;
                            let active = draft_new.read().is_none() && *current.read() == Some(pid);
                            rsx! {
                                button { key: "{pid}",
                                    class: if active { "rounded-md bg-accent px-3 py-2 text-left" } else { "rounded-md px-3 py-2 text-left hover:bg-accent/60" },
                                    onclick: move |_| { draft_new.set(None); current.set(Some(pid)); },
                                    p { class: "truncate text-sm font-medium", "{p.name}" }
                                    p { class: "truncate text-[11px] text-muted-foreground", "{p.ssh_label()}" }
                                }
                            }
                        }
                    }
                    if list.is_empty() && draft_new.read().is_none() { p { class: "px-3 py-2 text-xs text-muted-foreground", "No profiles yet." } }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, class: "mt-2",
                        onclick: move |_| draft_new.set(Some(Profile::new("New profile"))), "+ New profile" }
                }
                div { class: "min-h-0 min-w-0 flex-1 overflow-y-auto border-l pl-4",
                    match editing {
                        Some(p) => rsx! {
                            Editor { key: "{p.id}", profile: p.clone(), is_new: draft_new.read().is_some(),
                                on_saved: move |id| { draft_new.set(None); current.set(Some(id)); },
                                on_deleted: move |_| { draft_new.set(None); current.set(None); } }
                        },
                        None => rsx! { p { class: "p-6 text-sm text-muted-foreground", "Create a profile to get started." } },
                    }
                }
            }
        }
    }
}

#[component]
fn Editor(
    profile: Profile,
    is_new: bool,
    on_saved: EventHandler<ProfileId>,
    on_deleted: EventHandler<()>,
) -> Element {
    let state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let id = profile.id;
    let mut name = use_signal(|| profile.name.clone());
    let mut author = use_signal(|| profile.author_name.clone());
    let mut email = use_signal(|| profile.author_email.clone());
    let (initial_key, stored) = match &profile.ssh {
        SshAuth::KeyFile {
            path,
            has_passphrase,
        } => (Some(path.clone()), *has_passphrase),
        SshAuth::System => (None, false),
    };
    let mut use_key = use_signal(|| initial_key.is_some());
    let mut key_path = use_signal(|| initial_key.clone().unwrap_or_default());
    let mut key_rev = use_signal(|| 0u64);
    let mut passphrase = use_signal(|| None::<PassphraseChange>);
    let mut host = use_signal(|| "github.com".to_owned());
    let mut test = use_signal(|| None::<Result<String, String>>);
    let mut testing = use_signal(|| false);
    let mut public = use_signal(|| None::<String>);
    let mut confirm_delete = use_signal(|| false);
    let mut keys = use_signal(Vec::<String>::new);
    let svc = services.clone();
    use_future(move || {
        let svc = svc.clone();
        async move {
            let s = svc.clone();
            if let Ok(found) =
                commands::background(&svc, async move { s.discover_keys().await }).await
            {
                keys.set(found);
            }
        }
    });

    let build = move || -> Profile {
        let path = key_path.peek().trim().to_owned();
        Profile {
            id,
            name: name.peek().trim().to_owned(),
            author_name: author.peek().trim().to_owned(),
            author_email: email.peek().trim().to_owned(),
            ssh: if *use_key.peek() && !path.is_empty() {
                SshAuth::KeyFile {
                    path,
                    has_passphrase: stored,
                }
            } else {
                SshAuth::System
            },
        }
    };
    let preview = AppServices::ssh_command_preview(&build());
    let (s_save, s_del, s_test, s_gen, s_pub) = (
        services.clone(),
        services.clone(),
        services.clone(),
        services.clone(),
        services.clone(),
    );
    let pass_state = match (&*passphrase.read(), stored) {
        (Some(PassphraseChange::Set(_)), _) => "Will be saved to the keychain",
        (Some(PassphraseChange::Clear), _) => "Will be removed from the keychain",
        (_, true) => "Stored in the OS keychain",
        (_, false) => "None stored (fine for keys without a passphrase or keys in ssh-agent)",
    };
    let slug: String = name
        .read()
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
        .collect();
    let gen_path = format!("~/.ssh/ferrumgit_{slug}_ed25519");

    rsx! {
        div { class: "flex flex-col gap-4 text-sm",
            div { class: "grid grid-cols-2 gap-3",
                label { class: "col-span-2 flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Profile name" }
                    input { class: FIELD, initial_value: "{profile.name}", placeholder: "Work, Personal, Client X…", oninput: move |e| name.set(e.value()) }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Author name" }
                    input { class: FIELD, initial_value: "{profile.author_name}", placeholder: "Ada Lovelace", oninput: move |e| author.set(e.value()) }
                }
                label { class: "flex flex-col gap-1",
                    span { class: "text-xs text-muted-foreground", "Author email" }
                    input { class: FIELD, initial_value: "{profile.author_email}", placeholder: "ada@company.com", oninput: move |e| email.set(e.value()) }
                }
                p { class: "col-span-2 -mt-1 text-[11px] text-muted-foreground", "Leave both empty to use your global git identity." }
            }
            div { class: "flex flex-col gap-2 rounded-md border p-3",
                span { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "SSH" }
                label { class: "flex items-center gap-2",
                    input { r#type: "radio", name: "ssh-mode", checked: !*use_key.read(), onchange: move |_| use_key.set(false) }
                    "System SSH — ssh-agent and ~/.ssh/config decide"
                }
                label { class: "flex items-center gap-2",
                    input { r#type: "radio", name: "ssh-mode", checked: *use_key.read(), onchange: move |_| use_key.set(true) }
                    "Use a specific private key"
                }
                if *use_key.read() {
                    div { class: "flex flex-col gap-2 pl-6",
                        div { class: "flex gap-2",
                            for r in [*key_rev.read()] {
                                input { key: "{r}", class: "{FIELD} font-mono text-xs", placeholder: "~/.ssh/id_ed25519_work", initial_value: "{key_path.peek()}",
                                    oninput: move |e| { key_path.set(e.value()); public.set(None); } }
                            }
                            Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline,
                                onclick: move |_| { spawn(async move {
                                    if let Some(f) = rfd::AsyncFileDialog::new().set_title("Choose a private key").pick_file().await {
                                        key_path.set(f.path().display().to_string());
                                        key_rev += 1;
                                        public.set(None);
                                    }
                                }); },
                                "Browse…" }
                        }
                        if !keys.read().is_empty() {
                            div { class: "flex flex-wrap gap-1",
                                span { class: "text-[11px] text-muted-foreground", "Found in ~/.ssh:" }
                                for k in keys.read().clone() {
                                    {
                                        let label = k.rsplit('/').next().unwrap_or(&k).to_owned();
                                        rsx! { button { key: "{k}", class: "rounded border px-1.5 text-[11px] hover:bg-accent", title: "{k}",
                                            onclick: move |_| { key_path.set(k.clone()); key_rev += 1; public.set(None); }, "{label}" } }
                                    }
                                }
                            }
                        }
                        div { class: "flex flex-col gap-1",
                            span { class: "text-xs text-muted-foreground", "Key passphrase" }
                            div { class: "flex gap-2",
                                input { class: FIELD, r#type: "password", placeholder: if stored { "•••••••• (stored)" } else { "Optional" },
                                    autocomplete: "off",
                                    oninput: move |e| passphrase.set(Some(PassphraseChange::Set(Secret::new(e.value())))) }
                                if stored { Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| passphrase.set(Some(PassphraseChange::Clear)), "Forget" } }
                            }
                            span { class: "text-[11px] text-muted-foreground", "{pass_state}" }
                        }
                        div { class: "flex flex-wrap gap-2",
                            Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, disabled: key_path.read().trim().is_empty(),
                                onclick: move |_| {
                                    let (s, k) = (s_pub.clone(), key_path.peek().clone());
                                    spawn(async move {
                                        let s2 = s.clone();
                                        match run(&s, async move { s2.public_key(&k).await }).await {
                                            Ok(p) => public.set(Some(p)),
                                            Err(e) => state.notify(e, true),
                                        }
                                    });
                                },
                                "Show public key" }
                            Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, title: "Creates {gen_path} (ed25519, no passphrase)",
                                onclick: move |_| {
                                    let (s, path, comment) = (s_gen.clone(), gen_path.clone(), email.peek().clone());
                                    spawn(async move {
                                        let s2 = s.clone();
                                        let p2 = path.clone();
                                        match run(&s, async move { s2.generate_key(&p2, &comment).await }).await {
                                            Ok(pubkey) => {
                                                key_path.set(path);
                                                key_rev += 1;
                                                public.set(Some(pubkey));
                                                state.notify("Key generated. Add the public key to your Git host, then Save.", false);
                                            }
                                            Err(e) => state.notify(e, true),
                                        }
                                    });
                                },
                                "Generate new key" }
                        }
                        if let Some(pk) = public.read().clone() {
                            div { class: "flex items-start gap-2 rounded-md bg-muted p-2",
                                code { class: "min-w-0 flex-1 break-all text-[11px] select-text", "{pk}" }
                                Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline,
                                    onclick: move |_| { commands::copy(&pk); state.notify("Public key copied — paste it into GitHub/GitLab → SSH keys", false); }, "Copy" }
                            }
                        }
                    }
                }
                p { class: "font-mono text-[11px] text-muted-foreground", "GIT_SSH_COMMAND: {preview}" }
            }
            div { class: "flex flex-col gap-2 rounded-md border p-3",
                span { class: "text-xs font-semibold uppercase tracking-wide text-muted-foreground", "Test connection" }
                div { class: "flex gap-2",
                    input { class: FIELD, initial_value: "github.com", placeholder: "github.com, gitlab.com, git@host…", oninput: move |e| host.set(e.value()) }
                    Button { size: ButtonSize::Sm, variant: ButtonVariant::Outline, disabled: *testing.read(),
                        onclick: move |_| {
                            let (s, p, h) = (s_test.clone(), build(), host.peek().clone());
                            testing.set(true);
                            test.set(None);
                            spawn(async move {
                                let s2 = s.clone();
                                let r = run(&s, async move { s2.test_profile(&p, &h).await }).await;
                                testing.set(false);
                                test.set(Some(r));
                            });
                        },
                        if *testing.read() { "Testing…" } else { "Test" } }
                }
                p { class: "text-[11px] text-muted-foreground", "Runs ssh -T with this key (unsaved edits included). A host seen for the first time is trusted and added to known_hosts." }
                match test.read().clone() {
                    Some(Ok(m)) => rsx! { p { class: "whitespace-pre-wrap text-xs text-success select-text", "✓ {m}" } },
                    Some(Err(e)) => rsx! { p { class: "whitespace-pre-wrap text-xs text-destructive select-text", "✕ {e}" } },
                    None => rsx! {},
                }
            }
            div { class: "flex items-center gap-2",
                if !is_new {
                    if *confirm_delete.read() {
                        span { class: "text-xs text-destructive", "Repositories using it fall back to System default." }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Destructive,
                            onclick: move |_| { profiles::delete(state, s_del.clone(), id); on_deleted.call(()); }, "Delete profile" }
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, onclick: move |_| confirm_delete.set(false), "Keep" }
                    } else {
                        Button { size: ButtonSize::Sm, variant: ButtonVariant::Ghost, class: "text-destructive", onclick: move |_| confirm_delete.set(true), "Delete…" }
                    }
                }
                div { class: "flex-1" }
                Button { variant: ButtonVariant::Ghost, onclick: move |_| { let mut st = state; st.dialog.set(None::<Dialog>); }, "Close" }
                Button { disabled: name.read().trim().is_empty(),
                    onclick: move |_| {
                        let change = passphrase.peek().clone().unwrap_or(PassphraseChange::Keep);
                        passphrase.set(None);
                        profiles::save(state, s_save.clone(), build(), change, move |p| on_saved.call(p.id));
                    },
                    if is_new { "Create profile" } else { "Save" } }
            }
        }
    }
}

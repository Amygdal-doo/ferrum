//! Create/edit PostgreSQL connection profile. Passwords go to the OS vault
//! (or session memory) and are never written into the profile.

use crate::commands::{background, connection};
use crate::components::dialogs::Modal;
use crate::components::rust_ui::button::{Button, ButtonVariant};
use crate::components::rust_ui::input::{Input, InputType};
use crate::components::rust_ui::label::Label;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrum_application::{AppServices, PasswordChange};
use ferrum_domain::{ConnectionProfile, Endpoint, EnvironmentTag, Secret, TlsMode};

#[derive(Clone, PartialEq)]
enum TestState {
    Idle,
    Running,
    Ok(String),
    Err(String),
}

#[component]
pub fn ConnectionForm(existing: Option<ConnectionProfile>) -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let base = use_hook(|| {
        existing
            .clone()
            .unwrap_or_else(|| ConnectionProfile::new_postgres(""))
    });
    let (host0, port0) = match &base.endpoint {
        Endpoint::Tcp { host, port } => (host.clone(), port.to_string()),
        Endpoint::File { .. } => ("localhost".into(), "5432".into()),
    };
    let mut name = use_signal(|| base.display_name.clone());
    let mut host = use_signal(|| host0);
    let mut port = use_signal(|| port0);
    let mut database = use_signal(|| base.database.clone().unwrap_or_default());
    let mut username = use_signal(|| base.username.clone().unwrap_or_default());
    let mut password = use_signal(String::new);
    let mut remember = use_signal(|| existing.is_none() || base.secret_ref.is_some());
    let mut tls = use_signal(|| base.tls);
    let mut env = use_signal(|| base.env);
    let mut read_only = use_signal(|| base.read_only);
    let mut test = use_signal(|| TestState::Idle);
    let mut errors = use_signal(Vec::<String>::new);
    let mut saving = use_signal(|| false);
    let has_stored = base.secret_ref.is_some();

    let base_for_build = base.clone();
    let build = move || -> Result<ConnectionProfile, Vec<String>> {
        let mut p = base_for_build.clone();
        p.display_name = name.read().trim().to_owned();
        let port_num = port.read().trim().parse::<u16>().unwrap_or(0);
        p.endpoint = Endpoint::Tcp {
            host: host.read().trim().to_owned(),
            port: port_num,
        };
        p.database = Some(database.read().trim().to_owned()).filter(|s| !s.is_empty());
        p.username = Some(username.read().trim().to_owned()).filter(|s| !s.is_empty());
        p.tls = *tls.read();
        p.env = *env.read();
        p.read_only = *read_only.read();
        let errs = p.validate();
        if errs.is_empty() { Ok(p) } else { Err(errs) }
    };
    let typed_password = move || {
        Some(password.read().clone())
            .filter(|s| !s.is_empty())
            .map(Secret::new)
    };

    let svc_test = services.clone();
    let build_test = build.clone();
    let on_test = move |_| {
        let profile = match build_test() {
            Ok(p) => p,
            Err(e) => return errors.set(e),
        };
        errors.set(Vec::new());
        test.set(TestState::Running);
        let services = svc_test.clone();
        let pw = typed_password();
        spawn(async move {
            let svc = services.clone();
            let result = background(
                &services,
                async move { svc.connections.test(profile, pw).await },
            )
            .await;
            test.set(match result {
                Ok(Ok(info)) => TestState::Ok(info.version),
                Ok(Err(e)) => TestState::Err(match e.suggestion() {
                    Some(s) => format!("{} — {s}", e.message),
                    None => e.message,
                }),
                Err(e) => TestState::Err(e),
            });
        });
    };

    let svc_save = services.clone();
    let on_save = move |_| {
        let profile = match build() {
            Ok(p) => p,
            Err(e) => return errors.set(e),
        };
        let change = match (typed_password(), *remember.read()) {
            (Some(secret), remember) => PasswordChange::Set { secret, remember },
            (None, false) if has_stored => PasswordChange::Clear,
            (None, _) => PasswordChange::Keep,
        };
        saving.set(true);
        let services = svc_save.clone();
        spawn(async move {
            match connection::save(state, services, profile.clone(), change).await {
                Ok(saved) => {
                    // Bind the active tab to a newly created profile.
                    if let Some(tab_id) = *state.active_tab.peek() {
                        state.update_tab(tab_id, |t| {
                            if t.profile_id.is_none() {
                                t.profile_id = Some(saved.id);
                                t.unsaved = true;
                            }
                        });
                    }
                    state.dialog.set(None);
                }
                Err(e) => {
                    errors.set(vec![e]);
                    saving.set(false);
                }
            }
        });
    };

    let title = if existing.is_some() {
        "Edit connection"
    } else {
        "New PostgreSQL connection"
    };
    let pw_placeholder = if has_stored {
        "Stored in keychain — leave blank to keep"
    } else {
        "Password"
    };
    let field = "flex flex-col gap-1.5";
    let select_class = "h-9 rounded-md border bg-transparent px-2 text-sm";

    rsx! {
        Modal { title: title.to_owned(), on_close: move |_| state.dialog.set(None),
            div { class: "grid grid-cols-6 gap-3",
                div { class: "col-span-6 {field}",
                    Label { html_for: "cf-name", "Name" }
                    Input { id: "cf-name", value: name(), placeholder: "e.g. Local Postgres", oninput: move |e: FormEvent| name.set(e.value()) }
                }
                div { class: "col-span-4 {field}",
                    Label { html_for: "cf-host", "Host" }
                    Input { id: "cf-host", value: host(), oninput: move |e: FormEvent| host.set(e.value()) }
                }
                div { class: "col-span-2 {field}",
                    Label { html_for: "cf-port", "Port" }
                    Input { id: "cf-port", r#type: InputType::Number, value: port(), oninput: move |e: FormEvent| port.set(e.value()) }
                }
                div { class: "col-span-3 {field}",
                    Label { html_for: "cf-db", "Database" }
                    Input { id: "cf-db", value: database(), oninput: move |e: FormEvent| database.set(e.value()) }
                }
                div { class: "col-span-3 {field}",
                    Label { html_for: "cf-user", "User" }
                    Input { id: "cf-user", value: username(), oninput: move |e: FormEvent| username.set(e.value()) }
                }
                div { class: "col-span-6 {field}",
                    Label { html_for: "cf-pw", "Password" }
                    Input { id: "cf-pw", r#type: InputType::Password, value: password(), placeholder: pw_placeholder, oninput: move |e: FormEvent| password.set(e.value()) }
                    label { class: "flex items-center gap-2 text-xs text-muted-foreground",
                        input { r#type: "checkbox", checked: remember(), onchange: move |e| remember.set(e.checked()) }
                        "Save password in the OS keychain (otherwise kept for this session only)"
                    }
                }
                div { class: "col-span-3 {field}",
                    Label { html_for: "cf-tls", "TLS" }
                    select { id: "cf-tls", class: select_class,
                        onchange: move |e| if let Some(m) = TlsMode::ALL.iter().find(|m| format!("{m:?}") == e.value()) { tls.set(*m) },
                        for m in TlsMode::ALL {
                            option { value: "{m:?}", selected: m == tls(), "{m.label()}" }
                        }
                    }
                    if !tls().verifies_server() {
                        p { class: "text-[11px] text-warning-dark", "Server certificate is not verified." }
                    }
                }
                div { class: "col-span-3 {field}",
                    Label { html_for: "cf-env", "Environment" }
                    select { id: "cf-env", class: select_class,
                        onchange: move |e| if let Some(t) = EnvironmentTag::ALL.iter().find(|t| format!("{t:?}") == e.value()) { env.set(*t) },
                        for t in EnvironmentTag::ALL {
                            option { value: "{t:?}", selected: t == env(), "{t.label()}" }
                        }
                    }
                }
                label { class: "col-span-6 flex items-center gap-2 text-sm",
                    input { r#type: "checkbox", checked: read_only(), onchange: move |e| read_only.set(e.checked()) }
                    "Read-only session (server-enforced default; use a read-only role for real protection)"
                }
            }
            for e in errors.read().iter() {
                p { class: "mt-3 text-sm text-destructive", "{e}" }
            }
            match test() {
                TestState::Idle => rsx! {},
                TestState::Running => rsx! { p { class: "mt-3 text-sm text-muted-foreground", "Testing connection…" } },
                TestState::Ok(v) => rsx! { p { class: "mt-3 text-sm text-success", "Connected: {v}" } },
                TestState::Err(e) => rsx! { p { class: "mt-3 text-sm text-destructive select-text", "{e}" } },
            }
            div { class: "mt-5 flex justify-end gap-2",
                Button { variant: ButtonVariant::Outline, disabled: test() == TestState::Running, onclick: on_test, "Test connection" }
                Button { variant: ButtonVariant::Ghost, onclick: move |_| state.dialog.set(None), "Cancel" }
                Button { disabled: saving(), onclick: on_save, "Save" }
            }
        }
    }
}

//! Auth type select and fields (requests and collections).

use super::{FIELD, SecretInput};
use dioxus::prelude::*;
use ferrumapi_domain::{ApiKeyLocation, Auth};

const KINDS: [&str; 5] = ["inherit", "none", "basic", "bearer", "apikey"];

fn kind_of(a: &Auth) -> &'static str {
    match a {
        Auth::Inherit => KINDS[0],
        Auth::None => KINDS[1],
        Auth::Basic { .. } => KINDS[2],
        Auth::Bearer { .. } => KINDS[3],
        Auth::ApiKey { .. } => KINDS[4],
    }
}

fn blank(kind: &str) -> Auth {
    match kind {
        "inherit" => Auth::Inherit,
        "basic" => Auth::Basic {
            username: String::new(),
            password: String::new(),
        },
        "bearer" => Auth::Bearer {
            token: String::new(),
        },
        "apikey" => Auth::ApiKey {
            key: String::new(),
            value: String::new(),
            location: ApiKeyLocation::Header,
        },
        _ => Auth::None,
    }
}

fn label(kind: &str) -> &'static str {
    blank(kind).kind_label()
}

/// `scope` keys the fields (e.g. tab id) so switching owners remounts them.
/// `inherited` describes the auth that `Inherit` resolves to, if known.
#[component]
pub fn AuthEditor(
    auth: Auth,
    scope: String,
    allow_inherit: bool,
    #[props(default)] inherited: Option<String>,
    on_change: EventHandler<Auth>,
) -> Element {
    let kind = kind_of(&auth);
    let kinds: Vec<&str> = KINDS
        .iter()
        .copied()
        .filter(|k| allow_inherit || *k != "inherit")
        .collect();

    rsx! {
        div { class: "flex max-w-xl flex-col gap-3 text-sm",
            label { class: "flex items-center gap-3",
                span { class: "w-24 shrink-0 text-xs text-muted-foreground", "Type" }
                select {
                    class: "h-8 rounded-md border bg-background px-2 text-sm",
                    "aria-label": "Auth type",
                    onchange: move |e| on_change.call(blank(&e.value())),
                    for k in kinds {
                        option { key: "{k}", value: k, selected: k == kind, "{label(k)}" }
                    }
                }
            }
            // Single-item loop: the key re-mounts the uncontrolled fields.
            for k in [format!("{scope}-{kind}")] {
                AuthFields { key: "{k}", auth: auth.clone(), inherited: inherited.clone(), on_change }
            }
        }
    }
}

#[component]
fn AuthFields(auth: Auth, inherited: Option<String>, on_change: EventHandler<Auth>) -> Element {
    let (a1, a2) = (auth.clone(), auth.clone());
    rsx! {
        div { class: "flex flex-col gap-3",
            match auth.clone() {
                Auth::Inherit => rsx! {
                    p { class: "text-xs text-muted-foreground",
                        match &inherited {
                            Some(l) => rsx! { "Inherits " strong { class: "text-foreground", "{l}" } " from the parent folder or collection." },
                            None => rsx! { "This request is not in a collection, so no auth is sent." },
                        }
                    }
                },
                Auth::None => rsx! { p { class: "text-xs text-muted-foreground", "No authorization header is sent." } },
                Auth::Basic { username, password } => rsx! {
                    Field { name: "Username",
                        input { class: FIELD, spellcheck: "false", initial_value: "{username}",
                            oninput: move |e| if let Auth::Basic { password, .. } = &a1 { on_change.call(Auth::Basic { username: e.value(), password: password.clone() }) } }
                    }
                    Field { name: "Password",
                        SecretInput { value: password, label: "Password",
                            on_input: move |v: String| if let Auth::Basic { username, .. } = &a2 { on_change.call(Auth::Basic { username: username.clone(), password: v }) } }
                    }
                },
                Auth::Bearer { token } => rsx! {
                    Field { name: "Token",
                        SecretInput { value: token, label: "Token", placeholder: "Token",
                            on_input: move |v: String| on_change.call(Auth::Bearer { token: v }) }
                    }
                },
                Auth::ApiKey { key, value, location } => rsx! {
                    ApiKeyFields { key_name: key, value, location, current: auth.clone(), on_change }
                },
            }
        }
    }
}

#[component]
fn Field(name: String, children: Element) -> Element {
    rsx! {
        div { class: "flex items-center gap-3",
            span { class: "w-24 shrink-0 text-xs text-muted-foreground", "{name}" }
            div { class: "min-w-0 flex-1", {children} }
        }
    }
}

#[component]
fn ApiKeyFields(
    key_name: String,
    value: String,
    location: ApiKeyLocation,
    current: Auth,
    on_change: EventHandler<Auth>,
) -> Element {
    let set = move |f: &dyn Fn(&mut String, &mut String, &mut ApiKeyLocation)| {
        if let Auth::ApiKey {
            mut key,
            mut value,
            mut location,
        } = current.clone()
        {
            f(&mut key, &mut value, &mut location);
            on_change.call(Auth::ApiKey {
                key,
                value,
                location,
            });
        }
    };
    let (s1, s2, s3) = (set.clone(), set.clone(), set);
    rsx! {
        Field { name: "Key",
            input { class: FIELD, spellcheck: "false", placeholder: "X-API-Key", initial_value: "{key_name}",
                oninput: move |e| { let v = e.value(); s1(&|k, _, _| *k = v.clone()); } }
        }
        Field { name: "Value",
            SecretInput { value, label: "API key value",
                on_input: move |v: String| s2(&|_, val, _| *val = v.clone()) }
        }
        Field { name: "Add to",
            select { class: "h-8 rounded-md border bg-background px-2 text-sm", "aria-label": "API key location",
                onchange: move |e| {
                    let loc = if e.value() == "query" { ApiKeyLocation::Query } else { ApiKeyLocation::Header };
                    s3(&|_, _, l| *l = loc);
                },
                option { value: "header", selected: location == ApiKeyLocation::Header, "Header" }
                option { value: "query", selected: location == ApiKeyLocation::Query, "Query params" }
            }
        }
    }
}

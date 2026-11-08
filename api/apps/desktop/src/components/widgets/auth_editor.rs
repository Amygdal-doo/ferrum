//! Auth type select and fields (requests and collections).

use super::form_row;
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change, on_request_edit};
use crate::components::{hint, select};
use ferrumapi_domain::{ApiKeyLocation, Auth};
use gpui::{AnyElement, Context, SharedString, Window, prelude::*};
use std::rc::Rc;
use ui::{Label, LabelCommon as _, LabelSize, h_flex, v_flex};

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

/// Edits the auth being shown in place (the request's or a dialog draft's),
/// so field handlers always change the latest value.
pub type AuthTarget = Rc<dyn Fn(&mut FerrumApi, &mut dyn FnMut(&mut Auth))>;

/// What to render: the current auth plus where edits go.
pub struct AuthView {
    pub auth: Auth,
    /// Keys the fields (e.g. tab id) so switching owners swaps them.
    pub scope: String,
    pub allow_inherit: bool,
    /// The auth that `Inherit` resolves to, if known.
    pub inherited: Option<String>,
    /// Request auth fields send on Enter; dialog fields do not.
    pub request_fields: bool,
    pub target: AuthTarget,
}

impl FerrumApi {
    pub fn render_auth_editor(
        &mut self,
        view: AuthView,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let AuthView {
            auth,
            scope,
            allow_inherit,
            inherited,
            request_fields,
            target,
        } = view;
        let kind = kind_of(&auth);
        let options: Vec<(SharedString, &'static str)> = KINDS
            .iter()
            .copied()
            .filter(|k| allow_inherit || *k != "inherit")
            .map(|k| (blank(k).kind_label().into(), k))
            .collect();
        let type_select = {
            let target = target.clone();
            select(
                SharedString::from(format!("{scope}-auth-kind")),
                auth.kind_label(),
                options,
                move |this, k, _| target(this, &mut |a| *a = blank(k)),
                window,
                cx,
            )
        };

        // Builds a text field that writes into the current auth via `set`.
        let text_field = |this: &mut FerrumApi,
                          name: &str,
                          value: &str,
                          placeholder: &'static str,
                          set: fn(&mut Auth, String),
                          window: &mut Window,
                          cx: &mut Context<FerrumApi>| {
            let target = target.clone();
            let apply = move |this: &mut FerrumApi, text: String, _: &mut Context<FerrumApi>| {
                target(this, &mut |a| set(a, text.clone()))
            };
            let spec = FieldSpec::new(format!("{scope}:{kind}:{name}"))
                .value(value)
                .placeholder(placeholder)
                .mono();
            if request_fields {
                this.field(spec, on_request_edit(apply), window, cx)
                    .into_any_element()
            } else {
                this.field(spec, on_change(apply), window, cx)
                    .into_any_element()
            }
        };
        let secret_field = |this: &mut FerrumApi,
                            name: &str,
                            value: &str,
                            placeholder: &'static str,
                            set: fn(&mut Auth, String),
                            window: &mut Window,
                            cx: &mut Context<FerrumApi>| {
            let target = target.clone();
            let apply = move |this: &mut FerrumApi, text: String, _: &mut Context<FerrumApi>| {
                target(this, &mut |a| set(a, text.clone()))
            };
            let key = format!("{scope}:{kind}:{name}");
            if request_fields {
                this.render_secret_input(
                    key,
                    value,
                    placeholder,
                    on_request_edit(apply),
                    window,
                    cx,
                )
            } else {
                this.render_secret_input(key, value, placeholder, on_change(apply), window, cx)
            }
        };

        let fields: Vec<AnyElement> = match &auth {
            Auth::Inherit => vec![match &inherited {
                Some(label) => h_flex()
                    .gap_1()
                    .child(hint("Inherits"))
                    .child(Label::new(label.clone()).size(LabelSize::Small))
                    .child(hint("from the parent folder or collection."))
                    .into_any_element(),
                None => hint("This request is not in a collection, so no auth is sent.")
                    .into_any_element(),
            }],
            Auth::None => vec![hint("No authorization header is sent.").into_any_element()],
            Auth::Basic { username, password } => {
                let user = text_field(
                    self,
                    "username",
                    username,
                    "",
                    |a, v| {
                        if let Auth::Basic { username, .. } = a {
                            *username = v;
                        }
                    },
                    window,
                    cx,
                );
                let pass = secret_field(
                    self,
                    "password",
                    password,
                    "",
                    |a, v| {
                        if let Auth::Basic { password, .. } = a {
                            *password = v;
                        }
                    },
                    window,
                    cx,
                );
                vec![
                    form_row("Username", user).into_any_element(),
                    form_row("Password", pass).into_any_element(),
                ]
            }
            Auth::Bearer { token } => {
                let token = secret_field(
                    self,
                    "token",
                    token,
                    "Token",
                    |a, v| {
                        if let Auth::Bearer { token } = a {
                            *token = v;
                        }
                    },
                    window,
                    cx,
                );
                vec![form_row("Token", token).into_any_element()]
            }
            Auth::ApiKey {
                key,
                value,
                location,
            } => {
                let key_input = text_field(
                    self,
                    "key",
                    key,
                    "X-API-Key",
                    |a, v| {
                        if let Auth::ApiKey { key, .. } = a {
                            *key = v;
                        }
                    },
                    window,
                    cx,
                );
                let value_input = secret_field(
                    self,
                    "value",
                    value,
                    "",
                    |a, v| {
                        if let Auth::ApiKey { value, .. } = a {
                            *value = v;
                        }
                    },
                    window,
                    cx,
                );
                let location_select = {
                    let target = target.clone();
                    select(
                        SharedString::from(format!("{scope}-apikey-location")),
                        match location {
                            ApiKeyLocation::Header => "Header",
                            ApiKeyLocation::Query => "Query params",
                        },
                        vec![
                            ("Header".into(), ApiKeyLocation::Header),
                            ("Query params".into(), ApiKeyLocation::Query),
                        ],
                        move |this, loc, _| {
                            target(this, &mut |a| {
                                if let Auth::ApiKey { location, .. } = a {
                                    *location = loc;
                                }
                            })
                        },
                        window,
                        cx,
                    )
                };
                vec![
                    form_row("Key", key_input).into_any_element(),
                    form_row("Value", value_input).into_any_element(),
                    form_row("Add to", location_select).into_any_element(),
                ]
            }
        };

        v_flex()
            .max_w(gpui::px(576.))
            .gap_3()
            .child(form_row("Type", type_select))
            .children(fields)
            .into_any_element()
    }
}

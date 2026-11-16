//! Auth type select and fields (requests and collections). The OAuth /
//! Digest / AWS fields are in `auth_signing`.

use super::form_row;
use crate::app::FerrumApi;
use crate::components::fields::{FieldSpec, on_change, on_request_edit};
use crate::components::{hint, select};
use ferrum_ui::components::{Label, LabelCommon as _, LabelSize, h_flex, v_flex};
use ferrumapi_domain::{ApiKeyLocation, Auth, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, prelude::*};
use std::rc::Rc;

const KINDS: [&str; 9] = [
    "inherit", "none", "basic", "bearer", "apikey", "oauth2", "oauth1", "digest", "awsv4",
];

fn kind_of(a: &Auth) -> &'static str {
    match a {
        Auth::Inherit => KINDS[0],
        Auth::None => KINDS[1],
        Auth::Basic { .. } => KINDS[2],
        Auth::Bearer { .. } => KINDS[3],
        Auth::ApiKey { .. } => KINDS[4],
        Auth::OAuth2(_) => KINDS[5],
        Auth::OAuth1(_) => KINDS[6],
        Auth::Digest(_) => KINDS[7],
        Auth::AwsSigV4(_) => KINDS[8],
    }
}

/// A new auth of `kind`; the keychain-backed types get a fresh secret ref.
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
        "oauth2" => Auth::OAuth2(Box::default()),
        "oauth1" => Auth::OAuth1(Box::default()),
        "digest" => Auth::Digest(Box::default()),
        "awsv4" => Auth::AwsSigV4(Box::default()),
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
    /// Variables visible where the auth is used (OAuth 2 token requests).
    pub variables: VariableSet,
}

/// Builds the text and secret fields of one auth.
#[derive(Clone)]
pub(super) struct AuthFields {
    scope: String,
    kind: &'static str,
    request_fields: bool,
    pub(super) target: AuthTarget,
}

impl AuthFields {
    fn spec(&self, name: &str) -> String {
        format!("{}:{}:{name}", self.scope, self.kind)
    }

    /// A text field writing into the current auth via `set`.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn text(
        &self,
        this: &mut FerrumApi,
        name: &str,
        value: &str,
        placeholder: &'static str,
        set: fn(&mut Auth, String),
        window: &mut Window,
        cx: &mut Context<FerrumApi>,
    ) -> AnyElement {
        let target = self.target.clone();
        let apply = move |this: &mut FerrumApi, text: String, _: &mut Context<FerrumApi>| {
            target(this, &mut |a| set(a, text.clone()))
        };
        let spec = FieldSpec::new(self.spec(name))
            .value(value)
            .placeholder(placeholder)
            .mono();
        if self.request_fields {
            this.field(spec, on_request_edit(apply), window, cx)
                .into_any_element()
        } else {
            this.field(spec, on_change(apply), window, cx)
                .into_any_element()
        }
    }

    /// A masked field. Edits of keychain-backed fields are queued for the
    /// keychain (written by the autosave loop, never to disk).
    #[allow(clippy::too_many_arguments)]
    pub(super) fn secret(
        &self,
        this: &mut FerrumApi,
        name: &str,
        value: &str,
        placeholder: &'static str,
        set: fn(&mut Auth, String),
        window: &mut Window,
        cx: &mut Context<FerrumApi>,
    ) -> AnyElement {
        let target = self.target.clone();
        let apply = move |this: &mut FerrumApi, text: String, _: &mut Context<FerrumApi>| {
            let mut edited = None;
            target(this, &mut |a| {
                set(a, text.clone());
                edited = Some(a.clone());
            });
            if let Some(auth) = edited
                && let Some(account) = auth.secret_ref()
            {
                this.state.secret_edits.insert(account.to_owned(), auth);
            }
        };
        let key = self.spec(name);
        if self.request_fields {
            this.render_secret_input(key, value, placeholder, on_request_edit(apply), window, cx)
        } else {
            this.render_secret_input(key, value, placeholder, on_change(apply), window, cx)
        }
    }
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
            variables,
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
        let f = AuthFields {
            scope: scope.clone(),
            kind,
            request_fields,
            target: target.clone(),
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
                let user = f.text(
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
                let pass = f.secret(
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
                let token = f.secret(
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
                let key_input = f.text(
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
                let value_input = f.secret(
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
            Auth::OAuth2(_) | Auth::OAuth1(_) | Auth::Digest(_) | Auth::AwsSigV4(_) => {
                self.render_signing_fields(&auth, &f, &scope, variables, window, cx)
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

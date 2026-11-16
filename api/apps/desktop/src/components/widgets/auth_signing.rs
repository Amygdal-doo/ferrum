//! Fields of the OAuth 2.0, OAuth 1.0, Digest and AWS Signature auth types.
//! Their secrets are shown masked and kept in the OS keychain.

use super::auth_editor::AuthFields;
use super::form_row;
use crate::app::FerrumApi;
use crate::components::{checkbox, hint, select};
use ferrum_ui::components::{
    Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, h_flex,
};
use ferrumapi_domain::{Auth, ClientAuthentication, OAuth2Auth, OAuth2Grant, VariableSet};
use gpui::{AnyElement, Context, SharedString, Window, prelude::*};

/// Sets one field of the OAuth 2 config.
macro_rules! oauth2 {
    ($field:ident) => {
        |a: &mut Auth, v: String| {
            if let Auth::OAuth2(o) = a {
                o.$field = v;
            }
        }
    };
}

macro_rules! oauth1 {
    ($field:ident) => {
        |a: &mut Auth, v: String| {
            if let Auth::OAuth1(o) = a {
                o.$field = v;
            }
        }
    };
}

macro_rules! aws {
    ($field:ident) => {
        |a: &mut Auth, v: String| {
            if let Auth::AwsSigV4(o) = a {
                o.$field = v;
            }
        }
    };
}

fn keychain_hint() -> AnyElement {
    hint("Secrets are stored in the OS keychain, never in collection files or exports.")
        .into_any_element()
}

impl FerrumApi {
    pub(super) fn render_signing_fields(
        &mut self,
        auth: &Auth,
        f: &AuthFields,
        scope: &str,
        variables: VariableSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        match auth {
            Auth::OAuth2(o) => self.render_oauth2(o, f, scope, variables, window, cx),
            Auth::OAuth1(o) => vec![
                row(
                    self,
                    f,
                    "Consumer key",
                    "consumer_key",
                    &o.consumer_key,
                    oauth1!(consumer_key),
                    window,
                    cx,
                ),
                secret(
                    self,
                    f,
                    "Consumer secret",
                    "consumer_secret",
                    &o.consumer_secret,
                    oauth1!(consumer_secret),
                    window,
                    cx,
                ),
                row(
                    self,
                    f,
                    "Access token",
                    "token",
                    &o.token,
                    oauth1!(token),
                    window,
                    cx,
                ),
                secret(
                    self,
                    f,
                    "Token secret",
                    "token_secret",
                    &o.token_secret,
                    oauth1!(token_secret),
                    window,
                    cx,
                ),
                row(
                    self,
                    f,
                    "Realm",
                    "realm",
                    &o.realm,
                    oauth1!(realm),
                    window,
                    cx,
                ),
                hint("Signed with HMAC-SHA1 and sent in the Authorization header.")
                    .into_any_element(),
                keychain_hint(),
            ],
            Auth::Digest(d) => vec![
                row(
                    self,
                    f,
                    "Username",
                    "username",
                    &d.username,
                    |a, v| {
                        if let Auth::Digest(d) = a {
                            d.username = v;
                        }
                    },
                    window,
                    cx,
                ),
                secret(
                    self,
                    f,
                    "Password",
                    "password",
                    &d.password,
                    |a, v| {
                        if let Auth::Digest(d) = a {
                            d.password = v;
                        }
                    },
                    window,
                    cx,
                ),
                hint("The server's 401 challenge is answered once (MD5 or SHA-256, qop=auth).")
                    .into_any_element(),
                keychain_hint(),
            ],
            Auth::AwsSigV4(o) => vec![
                row(
                    self,
                    f,
                    "Access key",
                    "access_key_id",
                    &o.access_key_id,
                    aws!(access_key_id),
                    window,
                    cx,
                ),
                secret(
                    self,
                    f,
                    "Secret key",
                    "secret_access_key",
                    &o.secret_access_key,
                    aws!(secret_access_key),
                    window,
                    cx,
                ),
                row(
                    self,
                    f,
                    "Region",
                    "region",
                    &o.region,
                    aws!(region),
                    window,
                    cx,
                ),
                row(
                    self,
                    f,
                    "Service",
                    "service",
                    &o.service,
                    aws!(service),
                    window,
                    cx,
                ),
                secret(
                    self,
                    f,
                    "Session token",
                    "session_token",
                    &o.session_token,
                    aws!(session_token),
                    window,
                    cx,
                ),
                keychain_hint(),
            ],
            _ => Vec::new(),
        }
    }

    fn render_oauth2(
        &mut self,
        o: &OAuth2Auth,
        f: &AuthFields,
        scope: &str,
        variables: VariableSet,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let target = f.target.clone();
        let grant = select(
            SharedString::from(format!("{scope}-oauth2-grant")),
            match o.grant {
                OAuth2Grant::AuthorizationCode => "Authorization code (browser)",
                OAuth2Grant::ClientCredentials => "Client credentials",
            },
            vec![
                (
                    "Authorization code (browser)".into(),
                    OAuth2Grant::AuthorizationCode,
                ),
                ("Client credentials".into(), OAuth2Grant::ClientCredentials),
            ],
            move |this, g, _| {
                target(this, &mut |a| {
                    if let Auth::OAuth2(o) = a {
                        o.grant = g;
                    }
                });
            },
            window,
            cx,
        );
        let target = f.target.clone();
        let client_auth = select(
            SharedString::from(format!("{scope}-oauth2-client-auth")),
            match o.client_auth {
                ClientAuthentication::Body => "In the request body",
                ClientAuthentication::BasicHeader => "As a Basic Auth header",
            },
            vec![
                ("In the request body".into(), ClientAuthentication::Body),
                (
                    "As a Basic Auth header".into(),
                    ClientAuthentication::BasicHeader,
                ),
            ],
            move |this, c, _| {
                target(this, &mut |a| {
                    if let Auth::OAuth2(o) = a {
                        o.client_auth = c;
                    }
                });
            },
            window,
            cx,
        );
        let mut out = vec![form_row("Grant type", grant).into_any_element()];
        if o.grant == OAuth2Grant::AuthorizationCode {
            out.push(row(
                self,
                f,
                "Auth URL",
                "auth_url",
                &o.auth_url,
                oauth2!(auth_url),
                window,
                cx,
            ));
        }
        out.push(row(
            self,
            f,
            "Token URL",
            "token_url",
            &o.token_url,
            oauth2!(token_url),
            window,
            cx,
        ));
        out.push(row(
            self,
            f,
            "Client ID",
            "client_id",
            &o.client_id,
            oauth2!(client_id),
            window,
            cx,
        ));
        out.push(secret(
            self,
            f,
            "Client secret",
            "client_secret",
            &o.client_secret,
            oauth2!(client_secret),
            window,
            cx,
        ));
        out.push(row(
            self,
            f,
            "Scope",
            "scope",
            &o.scope,
            oauth2!(scope),
            window,
            cx,
        ));
        if o.grant == OAuth2Grant::AuthorizationCode {
            let port = if o.redirect_port == 0 {
                String::new()
            } else {
                o.redirect_port.to_string()
            };
            let port_field = f.text(
                self,
                "redirect_port",
                &port,
                "any free port",
                |a, v| {
                    if let Auth::OAuth2(o) = a {
                        o.redirect_port = v.trim().parse().unwrap_or(0);
                    }
                },
                window,
                cx,
            );
            let target = f.target.clone();
            let pkce = checkbox(
                SharedString::from(format!("{scope}-oauth2-pkce")),
                o.pkce,
                move |this, on, _| {
                    target(this, &mut |a| {
                        if let Auth::OAuth2(o) = a {
                            o.pkce = on;
                        }
                    });
                },
                cx,
            )
            .label("PKCE (S256)");
            out.push(form_row("Redirect port", port_field).into_any_element());
            out.push(
                hint(format!(
                    "Redirect URI: {}",
                    OAuth2Auth::redirect_uri(o.redirect_port).replace(":0/", ":<port>/")
                ))
                .into_any_element(),
            );
            out.push(form_row("Security", pkce).into_any_element());
        }
        out.push(form_row("Client auth", client_auth).into_any_element());
        out.push(row(
            self,
            f,
            "Header prefix",
            "header_prefix",
            &o.header_prefix,
            oauth2!(header_prefix),
            window,
            cx,
        ));

        let auth = Auth::OAuth2(Box::new(o.clone()));
        let secret_ref = o.secret_ref.clone();
        let get = Button::new(
            SharedString::from(format!("{scope}-oauth2-get")),
            "Get new access token",
        )
        .style(ButtonStyle::Filled)
        .size(ButtonSize::Compact)
        .on_click(cx.listener(move |this, _, _, cx| {
            this.oauth2_get_token(auth.clone(), variables.clone(), cx);
            cx.notify();
        }));
        let forget = {
            let secret_ref = secret_ref.clone();
            Button::new(
                SharedString::from(format!("{scope}-oauth2-forget")),
                "Forget token",
            )
            .style(ButtonStyle::Subtle)
            .size(ButtonSize::Compact)
            .on_click(cx.listener(move |this, _, _, cx| {
                this.oauth2_forget_token(secret_ref.clone(), cx);
                cx.notify();
            }))
        };
        out.push(h_flex().gap_2().child(get).child(forget).into_any_element());
        let status = self.state.oauth_status.get(&secret_ref).cloned().unwrap_or_else(|| {
            "Tokens are fetched when needed (client credentials, refresh) and cached per environment."
                .to_owned()
        });
        out.push(hint(status).into_any_element());
        out.push(keychain_hint());
        out
    }
}

/// A labelled text field.
#[allow(clippy::too_many_arguments)]
fn row(
    this: &mut FerrumApi,
    f: &AuthFields,
    label: &'static str,
    name: &str,
    value: &str,
    set: fn(&mut Auth, String),
    window: &mut Window,
    cx: &mut Context<FerrumApi>,
) -> AnyElement {
    let field = f.text(this, name, value, "", set, window, cx);
    form_row(label, field).into_any_element()
}

/// A labelled secret field.
#[allow(clippy::too_many_arguments)]
fn secret(
    this: &mut FerrumApi,
    f: &AuthFields,
    label: &'static str,
    name: &str,
    value: &str,
    set: fn(&mut Auth, String),
    window: &mut Window,
    cx: &mut Context<FerrumApi>,
) -> AnyElement {
    let field = f.secret(this, name, value, "", set, window, cx);
    form_row(label, field).into_any_element()
}

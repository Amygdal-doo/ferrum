//! Auth mapping. v2.1 stores parameters as `[{key, value, type}]`; v2.0 as a
//! plain object. Absent auth means "inherit from parent".
//!
//! OAuth 1/2, Digest and AWS secrets found in a file are imported (the app
//! moves them to the keychain when it saves); on export they are left
//! empty, so exported files never carry them.

use super::text;
use ferrumapi_domain::{
    ApiKeyLocation, Auth, AwsSigV4Auth, ClientAuthentication, DigestAuth, OAuth1Auth, OAuth2Auth,
    OAuth2Grant,
};
use serde_json::{Value, json};

/// Imported auth plus an optional note for unsupported auth types.
pub(super) fn import(v: Option<&Value>) -> (Auth, Option<String>) {
    let Some(obj) = v.filter(|v| v.is_object()) else {
        return (Auth::Inherit, None);
    };
    let kind = obj.get("type").and_then(Value::as_str).unwrap_or("");
    let p = |key: &str| param(obj.get(kind), key);
    let auth = match kind {
        "noauth" => Auth::None,
        "basic" => Auth::Basic {
            username: p("username"),
            password: p("password"),
        },
        "bearer" => Auth::Bearer { token: p("token") },
        "apikey" => Auth::ApiKey {
            key: p("key"),
            value: p("value"),
            location: if p("in") == "query" {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            },
        },
        "oauth2" => {
            let grant = p("grant_type");
            let mut a = OAuth2Auth {
                grant: if grant == "client_credentials" {
                    OAuth2Grant::ClientCredentials
                } else {
                    OAuth2Grant::AuthorizationCode
                },
                auth_url: p("authUrl"),
                token_url: p("accessTokenUrl"),
                client_id: p("clientId"),
                client_secret: p("clientSecret"),
                scope: p("scope"),
                client_auth: if p("client_authentication") == "body" {
                    ClientAuthentication::Body
                } else {
                    ClientAuthentication::BasicHeader
                },
                ..OAuth2Auth::default()
            };
            let prefix = p("headerPrefix");
            if !prefix.is_empty() {
                a.header_prefix = prefix;
            }
            Auth::OAuth2(Box::new(a))
        }
        "oauth1" => {
            let method = p("signatureMethod");
            let a = Auth::OAuth1(Box::new(OAuth1Auth {
                consumer_key: p("consumerKey"),
                consumer_secret: p("consumerSecret"),
                token: p("token"),
                token_secret: p("tokenSecret"),
                realm: p("realm"),
                ..OAuth1Auth::default()
            }));
            if !method.is_empty() && method != "HMAC-SHA1" {
                let note = format!(
                    "Imported from Postman: OAuth 1.0 signature method `{method}` is not supported; HMAC-SHA1 is used."
                );
                return (a, Some(note));
            }
            a
        }
        "digest" => Auth::Digest(Box::new(DigestAuth {
            username: p("username"),
            password: p("password"),
            ..DigestAuth::default()
        })),
        "awsv4" => {
            let mut a = AwsSigV4Auth {
                access_key_id: p("accessKey"),
                secret_access_key: p("secretKey"),
                service: p("service"),
                session_token: p("sessionToken"),
                ..AwsSigV4Auth::default()
            };
            let region = p("region");
            if !region.is_empty() {
                a.region = region;
            }
            Auth::AwsSigV4(Box::new(a))
        }
        "inherit" | "" => Auth::Inherit,
        other => {
            let note = format!(
                "Imported from Postman: auth type `{other}` is not supported, so this item inherits auth from its parent."
            );
            return (Auth::Inherit, Some(note));
        }
    };
    (auth, None)
}

/// Looks up `key` in the v2.1 array form or the v2.0 object form.
fn param(params: Option<&Value>, key: &str) -> String {
    match params {
        Some(Value::Array(rows)) => rows
            .iter()
            .find(|r| r.get("key").and_then(Value::as_str) == Some(key))
            .map(|r| text(r.get("value")))
            .unwrap_or_default(),
        Some(Value::Object(o)) => text(o.get(key)),
        _ => String::new(),
    }
}

/// `None` for [`Auth::Inherit`] (the field is omitted).
pub(super) fn export(auth: &Auth) -> Option<Value> {
    let rows = |pairs: &[(&str, &str)]| -> Value {
        pairs
            .iter()
            .map(|(k, v)| json!({ "key": k, "value": v, "type": "string" }))
            .collect()
    };
    Some(match auth {
        Auth::Inherit => return None,
        Auth::None => json!({ "type": "noauth" }),
        Auth::Basic { username, password } => json!({
            "type": "basic",
            "basic": rows(&[("username", username), ("password", password)]),
        }),
        Auth::Bearer { token } => json!({ "type": "bearer", "bearer": rows(&[("token", token)]) }),
        Auth::ApiKey {
            key,
            value,
            location,
        } => {
            let place = match location {
                ApiKeyLocation::Header => "header",
                ApiKeyLocation::Query => "query",
            };
            json!({
                "type": "apikey",
                "apikey": rows(&[("key", key), ("value", value), ("in", place)]),
            })
        }
        // Secrets stay in the keychain: exported empty.
        Auth::OAuth2(a) => {
            let grant = match a.grant {
                OAuth2Grant::AuthorizationCode if a.pkce => "authorization_code_with_pkce",
                OAuth2Grant::AuthorizationCode => "authorization_code",
                OAuth2Grant::ClientCredentials => "client_credentials",
            };
            let client_auth = match a.client_auth {
                ClientAuthentication::Body => "body",
                ClientAuthentication::BasicHeader => "header",
            };
            json!({
                "type": "oauth2",
                "oauth2": rows(&[
                    ("grant_type", grant),
                    ("authUrl", &a.auth_url),
                    ("accessTokenUrl", &a.token_url),
                    ("clientId", &a.client_id),
                    ("clientSecret", ""),
                    ("scope", &a.scope),
                    ("client_authentication", client_auth),
                    ("headerPrefix", &a.header_prefix),
                    ("addTokenTo", "header"),
                ]),
            })
        }
        Auth::OAuth1(a) => json!({
            "type": "oauth1",
            "oauth1": rows(&[
                ("consumerKey", &a.consumer_key),
                ("consumerSecret", ""),
                ("token", &a.token),
                ("tokenSecret", ""),
                ("signatureMethod", "HMAC-SHA1"),
                ("realm", &a.realm),
                ("addParamsToHeader", "true"),
            ]),
        }),
        Auth::Digest(a) => json!({
            "type": "digest",
            "digest": rows(&[("username", &a.username), ("password", "")]),
        }),
        Auth::AwsSigV4(a) => json!({
            "type": "awsv4",
            "awsv4": rows(&[
                ("accessKey", &a.access_key_id),
                ("secretKey", ""),
                ("region", &a.region),
                ("service", &a.service),
                ("sessionToken", ""),
            ]),
        }),
    })
}

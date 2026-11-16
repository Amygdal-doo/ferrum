//! Insomnia exports → a collection: the v4 format (JSON or YAML, a flat
//! `resources` list linked by `parentId`) and the v5 YAML format (nested
//! `collection` items). Request groups become folders; the base
//! environment's data becomes collection variables; `{{ _.name }}`
//! references become `{{name}}`. gRPC / WebSocket requests, sub-environments
//! and Insomnia template tags are noted in the description, not imported.

use crate::InteropError;
use ferrumapi_domain::urlparams::url_with_params;
use ferrumapi_domain::{
    ApiKeyLocation, Auth, AwsSigV4Auth, Collection, DigestAuth, Folder, FormField, FormFieldKind,
    HttpMethod, HttpRequest, Item, KeyValue, OAuth1Auth, OAuth2Auth, OAuth2Grant, RawLanguage,
    RequestBody, RequestKind, SavedRequest, Variable,
};
use serde_json::Value;

/// Whether `doc` looks like an Insomnia export.
#[must_use]
pub fn detect(doc: &Value) -> bool {
    doc.get("_type").and_then(Value::as_str) == Some("export")
        || doc
            .get("type")
            .and_then(Value::as_str)
            .is_some_and(|t| t.starts_with("collection.insomnia.rest"))
}

pub fn import(doc: &Value) -> Result<Collection, InteropError> {
    if !detect(doc) {
        return Err(InteropError::Unsupported("not an Insomnia export".into()));
    }
    let mut notes = Vec::new();
    let mut c = if let Some(resources) = doc.get("resources").and_then(Value::as_array) {
        v4(resources, &mut notes)
    } else {
        v5(doc, &mut notes)
    };
    if !notes.is_empty() {
        notes.dedup();
        if !c.description.is_empty() {
            c.description.push_str("\n\n");
        }
        c.description
            .push_str(&format!("Imported from Insomnia. {}", notes.join(" ")));
    }
    Ok(c)
}

fn s(v: &Value, k: &str) -> String {
    match v.get(k) {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(other) => other.to_string(),
    }
}

/// `{{ _.name }}` / `{{ name }}` → `{{name}}`. Template tags (`{% … %}`)
/// are left as written.
fn vars(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("{{") {
        let Some(len) = rest[start..].find("}}") else {
            break;
        };
        out.push_str(&rest[..start]);
        let inner = rest[start + 2..start + len].trim();
        let inner = inner.strip_prefix("_.").unwrap_or(inner);
        out.push_str(&format!("{{{{{inner}}}}}"));
        rest = &rest[start + len + 2..];
    }
    out.push_str(rest);
    out
}

/// The base environment's `data` as variables (nested values as JSON).
fn environment(data: Option<&Value>) -> Vec<Variable> {
    data.and_then(Value::as_object)
        .into_iter()
        .flatten()
        .map(|(k, v)| {
            let value = match v {
                Value::String(s) => vars(s),
                other => other.to_string(),
            };
            Variable::new(k.clone(), value)
        })
        .collect()
}

fn v4(resources: &[Value], notes: &mut Vec<String>) -> Collection {
    let of_type = |t: &'static str| {
        resources
            .iter()
            .filter(move |r| r.get("_type").and_then(Value::as_str) == Some(t))
    };
    let workspace = of_type("workspace").next();
    let mut c =
        Collection::new(workspace.map_or_else(|| "Insomnia import".to_owned(), |w| s(w, "name")));
    let root = workspace.map(|w| s(w, "_id")).unwrap_or_default();
    // The base environment is the one whose parent is the workspace.
    if let Some(env) = of_type("environment").find(|e| s(e, "parentId") == root) {
        c.variables = environment(env.get("data"));
    }
    if of_type("environment").count() > 1 {
        notes.push(
            "Sub-environments were not imported (only the base environment's variables).".into(),
        );
    }
    if of_type("workspace").count() > 1 {
        notes.push("Only the first workspace was imported.".into());
    }
    c.items = v4_children(resources, &root, notes, 0);
    c
}

fn v4_children(
    resources: &[Value],
    parent: &str,
    notes: &mut Vec<String>,
    depth: usize,
) -> Vec<Item> {
    if depth > 32 {
        return Vec::new();
    }
    let mut kids: Vec<&Value> = resources
        .iter()
        .filter(|r| s(r, "parentId") == parent)
        .collect();
    // Insomnia orders siblings by `metaSortKey`.
    kids.sort_by(|a, b| {
        let key = |v: &Value| v.get("metaSortKey").and_then(Value::as_f64).unwrap_or(0.0);
        key(a).total_cmp(&key(b))
    });
    kids.into_iter()
        .filter_map(|r| match r.get("_type").and_then(Value::as_str) {
            Some("request_group") => {
                let mut f = Folder::new(s(r, "name"));
                f.description = s(r, "description");
                f.auth = auth(r.get("authentication"), notes).unwrap_or(Auth::Inherit);
                f.items = v4_children(resources, &s(r, "_id"), notes, depth + 1);
                Some(Item::Folder(f))
            }
            Some("request") => Some(Item::Request(request(r, notes))),
            Some(other @ ("grpc_request" | "websocket_request" | "graphql_request")) => {
                notes.push(format!("`{other}` items were skipped."));
                None
            }
            _ => None,
        })
        .collect()
}

fn v5(doc: &Value, notes: &mut Vec<String>) -> Collection {
    let mut c = Collection::new(match s(doc, "name") {
        n if n.is_empty() => "Insomnia import".to_owned(),
        n => n,
    });
    if let Some(env) = doc.get("environments") {
        c.variables = environment(env.get("data"));
        if env
            .get("subEnvironments")
            .and_then(Value::as_array)
            .is_some_and(|a| !a.is_empty())
        {
            notes.push(
                "Sub-environments were not imported (only the base environment's variables)."
                    .into(),
            );
        }
    }
    c.items = v5_items(doc.get("collection"), notes, 0);
    c
}

fn v5_items(list: Option<&Value>, notes: &mut Vec<String>, depth: usize) -> Vec<Item> {
    if depth > 32 {
        return Vec::new();
    }
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|v| {
            if v.get("children").is_some() {
                let mut f = Folder::new(s(v, "name"));
                f.auth = auth(v.get("authentication"), notes).unwrap_or(Auth::Inherit);
                f.items = v5_items(v.get("children"), notes, depth + 1);
                Some(Item::Folder(f))
            } else if v.get("url").is_some() {
                Some(Item::Request(request(v, notes)))
            } else {
                None
            }
        })
        .collect()
}

fn rows(list: Option<&Value>) -> Vec<KeyValue> {
    list.and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|p| !s(p, "name").is_empty())
        .map(|p| {
            let mut kv = KeyValue::new(vars(&s(p, "name")), vars(&s(p, "value")));
            kv.enabled = !p.get("disabled").and_then(Value::as_bool).unwrap_or(false);
            kv.description = s(p, "description");
            kv
        })
        .collect()
}

fn request(r: &Value, notes: &mut Vec<String>) -> SavedRequest {
    let method_text = s(r, "method");
    let method = HttpMethod::parse(&method_text).unwrap_or_default();
    let mut req = HttpRequest::new(method, vars(&s(r, "url")));
    req.headers = rows(r.get("headers"));
    // Insomnia keeps query parameters apart from the URL.
    let extra = rows(r.get("parameters"));
    if !extra.is_empty() {
        req.params = ferrumapi_domain::urlparams::parse_query(&req.url);
        req.params.extend(extra);
        let base = req.url.split('?').next().unwrap_or_default().to_owned();
        req.url = url_with_params(&base, &req.params);
    } else {
        req.params = ferrumapi_domain::urlparams::parse_query(&req.url);
    }
    if let Some(a) = auth(r.get("authentication"), notes) {
        req.auth = a;
    }
    let body = r.get("body").cloned().unwrap_or(Value::Null);
    let mime = s(&body, "mimeType");
    let text = vars(&s(&body, "text"));
    req.body = match mime.as_str() {
        "" if text.is_empty() => RequestBody::None,
        "application/graphql" => {
            let doc: Value = serde_json::from_str(&text).unwrap_or(Value::Null);
            req.kind = RequestKind::Graphql;
            req.specs.graphql.query = s(&doc, "query");
            req.specs.graphql.variables = doc
                .get("variables")
                .filter(|v| !v.is_null())
                .map(|v| serde_json::to_string_pretty(v).unwrap_or_default())
                .unwrap_or_default();
            req.specs.graphql.operation_name = s(&doc, "operationName");
            RequestBody::None
        }
        "application/x-www-form-urlencoded" => RequestBody::UrlEncoded {
            fields: rows(body.get("params")),
        },
        "multipart/form-data" => RequestBody::FormData {
            fields: body
                .get("params")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .map(|p| {
                    let file = s(p, "type") == "file";
                    FormField {
                        key: vars(&s(p, "name")),
                        value: if file {
                            s(p, "fileName")
                        } else {
                            vars(&s(p, "value"))
                        },
                        kind: if file {
                            FormFieldKind::File
                        } else {
                            FormFieldKind::Text
                        },
                        enabled: !p.get("disabled").and_then(Value::as_bool).unwrap_or(false),
                    }
                })
                .collect(),
        },
        "application/octet-stream" => RequestBody::Binary {
            path: Some(s(&body, "fileName")).filter(|p| !p.is_empty()),
        },
        m => RequestBody::Raw {
            language: if m.contains("json") {
                RawLanguage::Json
            } else if m.contains("xml") {
                RawLanguage::Xml
            } else if m.contains("html") {
                RawLanguage::Html
            } else {
                RawLanguage::Text
            },
            text,
        },
    };
    if text_has_tags(r) {
        notes.push("Insomnia template tags ({% … %}) were kept as text.".into());
    }
    let mut saved = SavedRequest::new(s(r, "name"), req);
    saved.description = s(r, "description");
    saved
}

fn text_has_tags(r: &Value) -> bool {
    r.to_string().contains("{%")
}

/// `None` for inherited / absent auth.
fn auth(a: Option<&Value>, notes: &mut Vec<String>) -> Option<Auth> {
    let a = a.filter(|a| a.as_object().is_some_and(|o| !o.is_empty()))?;
    if a.get("disabled").and_then(Value::as_bool) == Some(true) {
        return Some(Auth::None);
    }
    let g = |k: &str| vars(&s(a, k));
    Some(match s(a, "type").as_str() {
        "none" => Auth::None,
        "basic" => Auth::Basic {
            username: g("username"),
            password: g("password"),
        },
        "bearer" => {
            let prefix = g("prefix");
            if prefix.is_empty() || prefix.eq_ignore_ascii_case("bearer") {
                Auth::Bearer { token: g("token") }
            } else {
                Auth::ApiKey {
                    key: "Authorization".into(),
                    value: format!("{prefix} {}", g("token")),
                    location: ApiKeyLocation::Header,
                }
            }
        }
        "apikey" => Auth::ApiKey {
            key: g("key"),
            value: g("value"),
            location: if s(a, "addTo") == "queryParams" {
                ApiKeyLocation::Query
            } else {
                ApiKeyLocation::Header
            },
        },
        "digest" => Auth::Digest(Box::new(DigestAuth {
            username: g("username"),
            password: g("password"),
            ..DigestAuth::default()
        })),
        "oauth1" => Auth::OAuth1(Box::new(OAuth1Auth {
            consumer_key: g("consumerKey"),
            consumer_secret: g("consumerSecret"),
            token: g("tokenKey"),
            token_secret: g("tokenSecret"),
            realm: g("realm"),
            ..OAuth1Auth::default()
        })),
        "oauth2" => {
            let grant = s(a, "grantType");
            let mut o = OAuth2Auth {
                auth_url: g("authorizationUrl"),
                token_url: g("accessTokenUrl"),
                client_id: g("clientId"),
                client_secret: g("clientSecret"),
                scope: g("scope"),
                pkce: a.get("usePkce").and_then(Value::as_bool).unwrap_or(true),
                ..OAuth2Auth::default()
            };
            match grant.as_str() {
                "client_credentials" => o.grant = OAuth2Grant::ClientCredentials,
                "authorization_code" | "" => {}
                other => notes.push(format!(
                    "OAuth 2.0 grant `{other}` is not supported; authorization code is used."
                )),
            }
            let prefix = g("tokenPrefix");
            if !prefix.is_empty() {
                o.header_prefix = prefix;
            }
            Auth::OAuth2(Box::new(o))
        }
        "iam" => {
            let mut aws = AwsSigV4Auth {
                access_key_id: g("accessKeyId"),
                secret_access_key: g("secretAccessKey"),
                session_token: g("sessionToken"),
                service: g("service"),
                ..AwsSigV4Auth::default()
            };
            let region = g("region");
            if !region.is_empty() {
                aws.region = region;
            }
            Auth::AwsSigV4(Box::new(aws))
        }
        other => {
            notes.push(format!(
                "Auth type `{other}` is not supported (left as inherit)."
            ));
            return None;
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn variable_syntax() {
        assert_eq!(
            vars("{{ _.base_url }}/a/{{id}}?x={{ _.k }}"),
            "{{base_url}}/a/{{id}}?x={{k}}"
        );
        assert_eq!(vars("{{ unclosed"), "{{ unclosed");
        assert_eq!(vars("{% now 'iso' %}"), "{% now 'iso' %}");
    }
}

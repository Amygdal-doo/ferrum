//! OpenAPI 3.0 / 3.1 (JSON or YAML) → a collection: one request per
//! operation, in a folder per first tag, against a `{{baseUrl}}` variable.
//! Path parameters become `{{name}}` variables, query and header parameters
//! rows (required ones enabled), request bodies examples (given or built
//! from the schema), security schemes auth. Each request's description
//! lists its expected responses with example bodies, so tests can be
//! drafted from it.

mod schema;

use crate::InteropError;
use ferrumapi_domain::urlparams::url_with_params;
use ferrumapi_domain::{
    ApiKeyLocation, Auth, Collection, DigestAuth, Folder, FormField, FormFieldKind, HttpMethod,
    HttpRequest, Item, KeyValue, OAuth2Auth, OAuth2Grant, RawLanguage, RequestBody, SavedRequest,
    Variable,
};
use schema::{example, resolve};
use serde_json::Value;

/// Longest example body kept in a request description.
const MAX_RESPONSE_EXAMPLE: usize = 2_000;

/// Builds a collection from an OpenAPI 3 document (already parsed).
pub fn import(doc: &Value) -> Result<Collection, InteropError> {
    match doc.get("openapi").and_then(Value::as_str) {
        Some(v) if v.starts_with('3') => {}
        _ if doc.get("swagger").is_some() => {
            return Err(InteropError::Unsupported(
                "Swagger 2.0 — convert it to OpenAPI 3 first".into(),
            ));
        }
        _ => {
            return Err(InteropError::Unsupported(
                "not an OpenAPI 3 document".into(),
            ));
        }
    }
    let info = doc.get("info");
    let text = |v: Option<&Value>, k: &str| {
        v.and_then(|v| v.get(k))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned()
    };
    let mut c = Collection::new(match text(info, "title") {
        t if t.is_empty() => "OpenAPI import".to_owned(),
        t => t,
    });
    c.description = text(info, "description");
    c.variables.push(Variable::new("baseUrl", base_url(doc)));
    c.auth = security(doc, doc.get("security")).unwrap_or(Auth::None);

    let methods = [
        ("get", HttpMethod::Get),
        ("post", HttpMethod::Post),
        ("put", HttpMethod::Put),
        ("patch", HttpMethod::Patch),
        ("delete", HttpMethod::Delete),
        ("head", HttpMethod::Head),
        ("options", HttpMethod::Options),
    ];
    let mut folders: Vec<Folder> = Vec::new();
    for (path, item) in doc
        .get("paths")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let item = resolve(doc, item);
        let shared = item.get("parameters");
        for (key, method) in methods {
            let Some(op) = item.get(key) else { continue };
            let saved = operation(doc, path, method, op, shared, &mut c.variables);
            let tag = op
                .get("tags")
                .and_then(Value::as_array)
                .and_then(|t| t.first())
                .and_then(Value::as_str);
            match tag {
                Some(tag) => {
                    let i = folders
                        .iter()
                        .position(|f| f.name == tag)
                        .unwrap_or_else(|| {
                            folders.push(Folder::new(tag));
                            folders.len() - 1
                        });
                    if let Some(folder) = folders.get_mut(i) {
                        folder.items.push(Item::Request(saved));
                    }
                }
                None => c.items.push(Item::Request(saved)),
            }
        }
    }
    let mut items: Vec<Item> = folders.into_iter().map(Item::Folder).collect();
    items.append(&mut c.items);
    c.items = items;
    Ok(c)
}

/// The first server's URL with its variables' defaults filled in.
fn base_url(doc: &Value) -> String {
    let Some(server) = doc
        .get("servers")
        .and_then(Value::as_array)
        .and_then(|s| s.first())
    else {
        return String::new();
    };
    let mut url = server
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned();
    for (name, var) in server
        .get("variables")
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        let default = var
            .get("default")
            .and_then(Value::as_str)
            .unwrap_or_default();
        url = url.replace(&format!("{{{name}}}"), default);
    }
    url.trim_end_matches('/').to_owned()
}

fn scalar(v: &Value) -> String {
    match v {
        Value::String(s) => s.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

fn operation(
    doc: &Value,
    path: &str,
    method: HttpMethod,
    op: &Value,
    shared: Option<&Value>,
    variables: &mut Vec<Variable>,
) -> SavedRequest {
    let str_of = |k: &str| op.get(k).and_then(Value::as_str).unwrap_or_default();
    let name = [str_of("summary"), str_of("operationId")]
        .into_iter()
        .find(|s| !s.trim().is_empty())
        .map_or_else(|| format!("{method} {path}"), str::to_owned);
    // `{id}` → `{{id}}`, a variable the user fills in.
    let url_path = path.replace('{', "{{").replace('}', "}}");
    let mut request = HttpRequest::new(method, format!("{{{{baseUrl}}}}{url_path}"));
    let params = shared
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .chain(
            op.get("parameters")
                .and_then(Value::as_array)
                .into_iter()
                .flatten(),
        );
    for p in params {
        let p = resolve(doc, p);
        let pname = p.get("name").and_then(Value::as_str).unwrap_or_default();
        if pname.is_empty() {
            continue;
        }
        let value = p
            .get("example")
            .map(scalar)
            .or_else(|| p.get("schema").map(|s| scalar(&example(doc, s))))
            .unwrap_or_default();
        let mut row = KeyValue::new(pname, value.clone());
        row.enabled = p.get("required").and_then(Value::as_bool).unwrap_or(false);
        row.description = p
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned();
        match p.get("in").and_then(Value::as_str) {
            Some("query") => {
                // Replace a row from the path item with the operation's own.
                request.params.retain(|r| r.key != pname);
                request.params.push(row);
            }
            Some("header") => {
                request.headers.retain(|r| r.key != pname);
                request.headers.push(row);
            }
            Some("path") if !variables.iter().any(|v| v.key == pname) => {
                variables.push(Variable::new(pname, value));
            }
            _ => {}
        }
    }
    request.url = url_with_params(&request.url, &request.params);
    if let Some(body) = op.get("requestBody").map(|b| resolve(doc, b)) {
        request.body = request_body(doc, body);
    }
    if let Some(auth) = security(doc, op.get("security")) {
        request.auth = auth;
    }
    let mut saved = SavedRequest::new(name, request);
    saved.description = describe(doc, op);
    saved
}

fn content_example(doc: &Value, media: &Value) -> Value {
    if let Some(v) = media.get("example") {
        return v.clone();
    }
    if let Some(v) = media
        .get("examples")
        .and_then(Value::as_object)
        .and_then(|e| e.values().next())
    {
        return resolve(doc, v).get("value").cloned().unwrap_or(Value::Null);
    }
    media.get("schema").map_or(Value::Null, |s| example(doc, s))
}

fn request_body(doc: &Value, body: &Value) -> RequestBody {
    let Some(content) = body.get("content").and_then(Value::as_object) else {
        return RequestBody::None;
    };
    let pick = |want: &str| content.iter().find(|(k, _)| k.contains(want));
    if let Some((_, media)) = pick("json") {
        let v = content_example(doc, media);
        return RequestBody::Raw {
            language: RawLanguage::Json,
            text: serde_json::to_string_pretty(&v).unwrap_or_default(),
        };
    }
    let props = |media: &Value| -> Vec<(String, Value)> {
        let s = media.get("schema").map(|s| resolve(doc, s));
        s.and_then(|s| s.get("properties"))
            .and_then(Value::as_object)
            .into_iter()
            .flatten()
            .map(|(k, v)| (k.clone(), resolve(doc, v).clone()))
            .collect()
    };
    if let Some((_, media)) = pick("x-www-form-urlencoded") {
        let fields = props(media)
            .into_iter()
            .map(|(k, s)| KeyValue::new(k, scalar(&example(doc, &s))))
            .collect();
        return RequestBody::UrlEncoded { fields };
    }
    if let Some((_, media)) = pick("multipart/form-data") {
        let fields = props(media)
            .into_iter()
            .map(|(k, s)| {
                let file = s.get("format").and_then(Value::as_str) == Some("binary");
                FormField {
                    key: k,
                    value: if file {
                        String::new()
                    } else {
                        scalar(&example(doc, &s))
                    },
                    kind: if file {
                        FormFieldKind::File
                    } else {
                        FormFieldKind::Text
                    },
                    enabled: true,
                }
            })
            .collect();
        return RequestBody::FormData { fields };
    }
    match content.iter().next() {
        Some((ct, media)) => RequestBody::Raw {
            language: if ct.contains("xml") {
                RawLanguage::Xml
            } else {
                RawLanguage::Text
            },
            text: scalar(&content_example(doc, media)),
        },
        None => RequestBody::None,
    }
}

/// The auth of the first usable requirement; `Some(Auth::None)` for an
/// explicit empty list (no auth), `None` when `security` is absent.
fn security(doc: &Value, security: Option<&Value>) -> Option<Auth> {
    let list = security?.as_array()?;
    if list.is_empty() {
        return Some(Auth::None);
    }
    let schemes = doc.pointer("/components/securitySchemes");
    list.iter()
        .filter_map(Value::as_object)
        .flat_map(|req| req.keys())
        .find_map(|name| scheme_auth(resolve(doc, schemes?.get(name)?), name))
}

fn scheme_auth(s: &Value, name: &str) -> Option<Auth> {
    let get = |k: &str| s.get(k).and_then(Value::as_str).unwrap_or_default();
    Some(
        match (get("type"), get("scheme").to_ascii_lowercase().as_str()) {
            ("http", "basic") => Auth::Basic {
                username: "{{username}}".into(),
                password: "{{password}}".into(),
            },
            ("http", "bearer") => Auth::Bearer {
                token: "{{token}}".into(),
            },
            ("http", "digest") => Auth::Digest(Box::new(DigestAuth {
                username: "{{username}}".into(),
                password: "{{password}}".into(),
                ..DigestAuth::default()
            })),
            ("apiKey", _) => Auth::ApiKey {
                key: get("name").to_owned(),
                value: format!("{{{{{name}}}}}"),
                location: if get("in") == "query" {
                    ApiKeyLocation::Query
                } else {
                    ApiKeyLocation::Header
                },
            },
            ("oauth2", _) => {
                let flows = s.get("flows")?;
                let flow_url = |flow: &Value, k: &str| {
                    flow.get(k)
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_owned()
                };
                let scopes = |flow: &Value| {
                    flow.get("scopes")
                        .and_then(Value::as_object)
                        .map(|s| s.keys().cloned().collect::<Vec<_>>().join(" "))
                        .unwrap_or_default()
                };
                let mut a = OAuth2Auth {
                    client_id: "{{clientId}}".into(),
                    ..OAuth2Auth::default()
                };
                if let Some(f) = flows.get("authorizationCode") {
                    a.auth_url = flow_url(f, "authorizationUrl");
                    a.token_url = flow_url(f, "tokenUrl");
                    a.scope = scopes(f);
                } else {
                    let f = flows.get("clientCredentials")?;
                    a.grant = OAuth2Grant::ClientCredentials;
                    a.token_url = flow_url(f, "tokenUrl");
                    a.scope = scopes(f);
                }
                Auth::OAuth2(Box::new(a))
            }
            _ => return None,
        },
    )
}

/// The operation's description plus its expected responses.
fn describe(doc: &Value, op: &Value) -> String {
    let mut out = op
        .get("description")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_owned();
    let Some(responses) = op.get("responses").and_then(Value::as_object) else {
        return out;
    };
    if !out.is_empty() {
        out.push_str("\n\n");
    }
    out.push_str("Expected responses (OpenAPI):");
    for (code, resp) in responses {
        let resp = resolve(doc, resp);
        let summary = resp
            .get("description")
            .and_then(Value::as_str)
            .unwrap_or_default();
        out.push_str(&format!("\n- {code}: {summary}"));
        if let Some((ct, media)) = resp
            .get("content")
            .and_then(Value::as_object)
            .and_then(|c| c.iter().next())
        {
            let ex = serde_json::to_string(&content_example(doc, media)).unwrap_or_default();
            let ex: String = ex.chars().take(MAX_RESPONSE_EXAMPLE).collect();
            out.push_str(&format!("\n  {ct}: {ex}"));
        }
    }
    out
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;

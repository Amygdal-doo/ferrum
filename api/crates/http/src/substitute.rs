//! `{{variable}}` substitution and unresolved-variable detection across every
//! user-editable field of a request.

use ferrumapi_domain::{Auth, HttpRequest, KeyValue, RequestBody, VariableSet};

/// Returns a copy with `{{variables}}` substituted in URL, params, headers,
/// body (raw text, urlencoded and form-data text fields; file paths too) and auth.
#[must_use]
pub fn substitute(request: &HttpRequest, vars: &VariableSet) -> HttpRequest {
    let r = |s: &str| vars.resolve(s);
    let kvs = |rows: &[KeyValue]| -> Vec<KeyValue> {
        rows.iter()
            .map(|kv| KeyValue {
                key: r(&kv.key),
                value: r(&kv.value),
                ..kv.clone()
            })
            .collect()
    };
    let body = match &request.body {
        RequestBody::None => RequestBody::None,
        RequestBody::Raw { language, text } => RequestBody::Raw {
            language: *language,
            text: r(text),
        },
        RequestBody::UrlEncoded { fields } => RequestBody::UrlEncoded {
            fields: kvs(fields),
        },
        RequestBody::FormData { fields } => RequestBody::FormData {
            fields: fields
                .iter()
                .map(|f| {
                    let mut f = f.clone();
                    f.key = r(&f.key);
                    f.value = r(&f.value);
                    f
                })
                .collect(),
        },
        RequestBody::Binary { path } => RequestBody::Binary {
            path: path.as_deref().map(r),
        },
    };
    let auth = match &request.auth {
        Auth::Inherit => Auth::Inherit,
        Auth::None => Auth::None,
        Auth::Basic { username, password } => Auth::Basic {
            username: r(username),
            password: r(password),
        },
        Auth::Bearer { token } => Auth::Bearer { token: r(token) },
        Auth::ApiKey {
            key,
            value,
            location,
        } => Auth::ApiKey {
            key: r(key),
            value: r(value),
            location: *location,
        },
    };
    HttpRequest {
        method: request.method,
        url: r(&request.url),
        params: kvs(&request.params),
        headers: kvs(&request.headers),
        body,
        auth,
        settings: request.settings,
    }
}

/// Every unresolved variable name referenced anywhere in `request` (enabled
/// rows only), deduplicated, in first-seen order.
#[must_use]
pub fn unresolved_variables(request: &HttpRequest, vars: &VariableSet) -> Vec<String> {
    let mut texts: Vec<&str> = vec![request.url.as_str()];
    for kv in request
        .params
        .iter()
        .chain(&request.headers)
        .filter(|kv| kv.enabled)
    {
        texts.push(&kv.key);
        texts.push(&kv.value);
    }
    match &request.body {
        RequestBody::None => {}
        RequestBody::Raw { text, .. } => texts.push(text),
        RequestBody::UrlEncoded { fields } => {
            for kv in fields.iter().filter(|kv| kv.enabled) {
                texts.push(&kv.key);
                texts.push(&kv.value);
            }
        }
        RequestBody::FormData { fields } => {
            for f in fields.iter().filter(|f| f.enabled) {
                texts.push(&f.key);
                texts.push(&f.value);
            }
        }
        RequestBody::Binary { path } => texts.extend(path.as_deref()),
    }
    match &request.auth {
        Auth::Inherit | Auth::None => {}
        Auth::Basic { username, password } => texts.extend([username.as_str(), password]),
        Auth::Bearer { token } => texts.push(token),
        Auth::ApiKey { key, value, .. } => texts.extend([key.as_str(), value]),
    }
    let mut out: Vec<String> = Vec::new();
    for name in texts.into_iter().flat_map(|t| vars.unresolved(t)) {
        if !out.contains(&name) {
            out.push(name);
        }
    }
    out
}

#[cfg(test)]
#[path = "substitute_tests.rs"]
mod tests;

//! [`Parsed`] options → [`HttpRequest`]: method, URL, headers, auth, settings.

use super::body;
use super::options::Parsed;
use crate::InteropError;
use base64::Engine as _;
use ferrumapi_domain::urlparams::params_from_url;
use ferrumapi_domain::{Auth, HttpMethod, HttpRequest, KeyValue, RequestBody, RequestSettings};

pub(super) fn request(p: Parsed) -> Result<HttpRequest, InteropError> {
    let mut url = p
        .urls
        .first()
        .cloned()
        .ok_or_else(|| InteropError::Parse("no URL found in the curl command".into()))?;
    if !url.contains("://") && !url.starts_with("{{") {
        url = format!("http://{url}");
    }
    let mut headers: Vec<KeyValue> = p.headers.iter().filter_map(|h| header(h)).collect();
    let cookies: Vec<&str> = p
        .cookies
        .iter()
        .filter(|c| c.contains('='))
        .map(String::as_str)
        .collect();
    if !cookies.is_empty() {
        headers.push(KeyValue::new("Cookie", cookies.join("; ")));
    }
    if p.json {
        for (k, v) in [
            ("Content-Type", "application/json"),
            ("Accept", "application/json"),
        ] {
            if find(&headers, k).is_none() {
                headers.push(KeyValue::new(k, v));
            }
        }
    }

    let mut req_body = RequestBody::None;
    if p.get {
        url = body::append_query(&url, &body::query_from_data(&p.data));
    } else if !p.form.is_empty() {
        req_body = body::form(&p.form);
    } else if let Some(path) = &p.upload {
        req_body = RequestBody::Binary {
            path: Some(path.clone()),
        };
    } else if !p.data.is_empty() {
        let ct = find(&headers, "content-type").map(|i| headers[i].value.clone());
        req_body = body::from_data(&p.data, ct.as_deref());
    }
    body::drop_implied_content_type(&mut headers, &req_body);

    let method = match &p.method {
        Some(m) => HttpMethod::parse(m)
            .ok_or_else(|| InteropError::Parse(format!("unsupported HTTP method `{m}`")))?,
        None if p.head => HttpMethod::Head,
        None if p.get => HttpMethod::Get,
        None if p.upload.is_some() => HttpMethod::Put,
        None if !p.data.is_empty() || !p.form.is_empty() => HttpMethod::Post,
        None => HttpMethod::Get,
    };

    let auth = auth(&p, &mut headers);
    let mut settings = RequestSettings {
        follow_redirects: p.location,
        verify_tls: !p.insecure,
        ..RequestSettings::default()
    };
    if let Some(t) = &p.max_time {
        let secs: f64 = t
            .trim()
            .parse()
            .ok()
            .filter(|s: &f64| s.is_finite() && *s >= 0.0)
            .ok_or_else(|| InteropError::Parse(format!("invalid --max-time `{t}`")))?;
        settings.timeout_ms = (secs * 1000.0).round() as u64;
    }

    Ok(HttpRequest {
        method,
        params: params_from_url(&url, &[]),
        url,
        headers,
        body: req_body,
        auth,
        settings,
        ..HttpRequest::default()
    })
}

/// `Name: value`; `Name;` is an empty header; `Name:` (removal) and `@file` are skipped.
fn header(raw: &str) -> Option<KeyValue> {
    if raw.starts_with('@') {
        return None;
    }
    match raw.split_once(':') {
        Some((k, v)) if !v.trim().is_empty() => Some(KeyValue::new(k.trim(), v.trim())),
        Some(_) => None,
        None => raw.strip_suffix(';').map(|k| KeyValue::new(k.trim(), "")),
    }
}

pub(super) fn find(headers: &[KeyValue], name: &str) -> Option<usize> {
    headers
        .iter()
        .position(|h| h.key.eq_ignore_ascii_case(name))
}

/// `-u` wins, then `--oauth2-bearer`, then an `Authorization` header
/// (Bearer / decodable Basic), which is then removed from the headers.
fn auth(p: &Parsed, headers: &mut Vec<KeyValue>) -> Auth {
    if let Some(user) = &p.user {
        let (username, password) = user.split_once(':').unwrap_or((user, ""));
        return Auth::Basic {
            username: username.to_owned(),
            password: password.to_owned(),
        };
    }
    if let Some(token) = &p.bearer {
        return Auth::Bearer {
            token: token.clone(),
        };
    }
    let Some(i) = find(headers, "authorization") else {
        return Auth::None;
    };
    let value = headers[i].value.trim();
    let (scheme, cred) = value.split_once(' ').unwrap_or((value, ""));
    let cred = cred.trim();
    let auth = if scheme.eq_ignore_ascii_case("bearer") && !cred.is_empty() {
        Auth::Bearer {
            token: cred.to_owned(),
        }
    } else if scheme.eq_ignore_ascii_case("basic") {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(cred)
            .ok()
            .and_then(|b| String::from_utf8(b).ok());
        match decoded.as_deref().and_then(|d| d.split_once(':')) {
            Some((u, pw)) => Auth::Basic {
                username: u.to_owned(),
                password: pw.to_owned(),
            },
            None => return Auth::None,
        }
    } else {
        return Auth::None;
    };
    headers.remove(i);
    auth
}

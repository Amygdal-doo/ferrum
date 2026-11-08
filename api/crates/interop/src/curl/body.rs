//! Body inference for `-d` / `--data-urlencode` / `-F` arguments.

use super::options::Data;
use ferrumapi_domain::{FormField, FormFieldKind, KeyValue, RawLanguage, RequestBody};
use url::form_urlencoded;

const FORM_URLENCODED: &str = "application/x-www-form-urlencoded";

/// `-d` family → body. `content_type` is the explicit `Content-Type` header.
pub(super) fn from_data(data: &[Data], content_type: Option<&str>) -> RequestBody {
    if let Some(path) = data.iter().find_map(|d| match d {
        Data::File(p) => Some(p.clone()),
        _ => None,
    }) {
        return RequestBody::Binary { path: Some(path) };
    }
    if data.iter().any(|d| matches!(d, Data::Encode(_))) {
        let fields = data
            .iter()
            .flat_map(|d| match d {
                Data::Raw(s) => decode_pairs(s),
                Data::Encode(s) => vec![encode_field(s)],
                Data::File(_) => Vec::new(),
            })
            .collect();
        return RequestBody::UrlEncoded { fields };
    }
    let joined = data
        .iter()
        .filter_map(|d| match d {
            Data::Raw(s) => Some(s.as_str()),
            _ => None,
        })
        .collect::<Vec<_>>()
        .join("&");
    let raw = |language| RequestBody::Raw {
        language,
        text: joined.clone(),
    };
    match content_type.map(str::to_ascii_lowercase) {
        Some(ct) if ct.contains("json") => raw(RawLanguage::Json),
        Some(ct) if ct.contains(FORM_URLENCODED) => RequestBody::UrlEncoded {
            fields: decode_pairs(&joined),
        },
        Some(ct) if ct.contains("xml") => raw(RawLanguage::Xml),
        Some(ct) if ct.contains("html") => raw(RawLanguage::Html),
        Some(_) => raw(RawLanguage::Text),
        None if is_json(&joined) => raw(RawLanguage::Json),
        // curl sends `-d` data as application/x-www-form-urlencoded by default.
        None if looks_like_form(&joined) => RequestBody::UrlEncoded {
            fields: decode_pairs(&joined),
        },
        None => raw(RawLanguage::Text),
    }
}

fn is_json(s: &str) -> bool {
    serde_json::from_str::<serde_json::Value>(s.trim()).is_ok_and(|v| v.is_object() || v.is_array())
}

fn looks_like_form(s: &str) -> bool {
    !s.is_empty()
        && !s.contains(char::is_whitespace)
        && s.split('&')
            .all(|p| p.split_once('=').is_some_and(|(k, _)| !k.is_empty()))
}

/// `a=1&b=x%20y` → decoded rows.
fn decode_pairs(s: &str) -> Vec<KeyValue> {
    form_urlencoded::parse(s.as_bytes())
        .map(|(k, v)| KeyValue::new(k, v))
        .collect()
}

/// `--data-urlencode` argument: `name=content` or bare `content` (`=content`).
fn encode_field(arg: &str) -> KeyValue {
    match arg.split_once('=') {
        Some((name, content)) if !name.is_empty() => KeyValue::new(name, content),
        Some((_, content)) => KeyValue::new(content, ""),
        None => KeyValue::new(arg, ""),
    }
}

fn encode(s: &str) -> String {
    form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

/// `-G`: data arguments → a query string (raw data verbatim, urlencode encoded).
pub(super) fn query_from_data(data: &[Data]) -> String {
    data.iter()
        .filter_map(|d| match d {
            Data::Raw(s) => Some(s.clone()),
            Data::Encode(s) => Some(match s.split_once('=') {
                Some((name, content)) if !name.is_empty() => format!("{name}={}", encode(content)),
                Some((_, content)) => encode(content),
                None => encode(s),
            }),
            Data::File(_) => None,
        })
        .filter(|s| !s.is_empty())
        .collect::<Vec<_>>()
        .join("&")
}

/// Appends `query` to `url` (before any `#fragment`).
pub(crate) fn append_query(url: &str, query: &str) -> String {
    if query.is_empty() {
        return url.to_owned();
    }
    let (base, frag) = match url.find('#') {
        Some(i) => url.split_at(i),
        None => (url, ""),
    };
    let sep = match base.find('?') {
        None => "?",
        Some(_) if base.ends_with('?') || base.ends_with('&') => "",
        Some(_) => "&",
    };
    format!("{base}{sep}{query}{frag}")
}

/// `-F name=value`, `-F name=@path;type=...`, `--form-string name=value`.
pub(super) fn form(args: &[(String, bool)]) -> RequestBody {
    let fields = args
        .iter()
        .map(|(arg, literal)| {
            let (key, value) = arg.split_once('=').unwrap_or((arg, ""));
            let mut field = FormField {
                key: key.to_owned(),
                ..FormField::default()
            };
            if *literal {
                field.value = value.to_owned();
            } else if let Some(path) = value.strip_prefix('@') {
                field.kind = FormFieldKind::File;
                field.value = unquote(strip_params(path));
            } else {
                field.value = unquote(strip_params(value));
            }
            field
        })
        .collect();
    RequestBody::FormData { fields }
}

/// Drops `;type=...` / `;filename=...` / `;headers=...` suffixes (outside quotes).
fn strip_params(v: &str) -> &str {
    if let Some(end) = v.strip_prefix('"').and_then(|rest| rest.find('"')) {
        return &v[..end + 2];
    }
    [";type=", ";filename=", ";headers=", ";encoder="]
        .iter()
        .filter_map(|p| v.find(p))
        .min()
        .map_or(v, |i| &v[..i])
}

fn unquote(v: &str) -> String {
    match v.strip_prefix('"').and_then(|s| s.strip_suffix('"')) {
        Some(inner) => inner.replace("\\\"", "\"").replace("\\\\", "\\"),
        None => v.to_owned(),
    }
}

/// Removes a `Content-Type` header that just restates what the body implies
/// (the app adds it automatically), including multipart boundaries.
pub(super) fn drop_implied_content_type(headers: &mut Vec<KeyValue>, body: &RequestBody) {
    let implied = |v: &str| match body {
        RequestBody::Raw { language, .. } => v.eq_ignore_ascii_case(language.content_type()),
        RequestBody::UrlEncoded { .. } => v.eq_ignore_ascii_case(FORM_URLENCODED),
        RequestBody::FormData { .. } => v.to_ascii_lowercase().starts_with("multipart/form-data"),
        RequestBody::None | RequestBody::Binary { .. } => false,
    };
    headers.retain(|h| !(h.key.eq_ignore_ascii_case("content-type") && implied(h.value.trim())));
}

//! Python `requests` snippets.

use super::{Prepared, timeout_secs};
use ferrumapi_domain::{FormFieldKind, RawLanguage, RequestBody};
use serde_json::Value;
use std::fmt::Write as _;

pub(super) fn generate(p: &Prepared<'_>) -> String {
    let mut out = String::from("import requests\n\n");
    let _ = writeln!(out, "url = {}\n", py_str(&p.url));
    let mut headers = p.headers.clone();
    let mut kwargs: Vec<String> = Vec::new();
    let mut blocks: Vec<String> = Vec::new();

    match p.body {
        RequestBody::None | RequestBody::Binary { path: None } => {}
        RequestBody::Raw { language, text } if !text.is_empty() => {
            let json = (*language == RawLanguage::Json)
                .then(|| serde_json::from_str::<Value>(text).ok())
                .flatten();
            if let Some(v) = json {
                blocks.push(format!("payload = {}", py_value(&v, 0)));
                kwargs.push("json=payload".into());
            } else {
                if let Some(ct) = p.implied_content_type() {
                    headers.push(("Content-Type".into(), ct.into()));
                }
                blocks.push(format!("payload = {}", py_str(text)));
                kwargs.push("data=payload".into());
            }
        }
        RequestBody::Raw { .. } => {}
        RequestBody::UrlEncoded { fields } => {
            let pairs: Vec<(&str, &str)> = fields
                .iter()
                .filter(|f| f.is_active())
                .map(|f| (f.key.as_str(), f.value.as_str()))
                .collect();
            if !pairs.is_empty() {
                blocks.push(format!("payload = {}", py_pairs(&pairs)));
                kwargs.push("data=payload".into());
            }
        }
        RequestBody::FormData { fields } => {
            let active = fields.iter().filter(|f| f.enabled && !f.key.is_empty());
            let (files, texts): (Vec<_>, Vec<_>) =
                active.partition(|f| f.kind == FormFieldKind::File);
            if !texts.is_empty() {
                let pairs: Vec<(&str, &str)> = texts
                    .iter()
                    .map(|f| (f.key.as_str(), f.value.as_str()))
                    .collect();
                blocks.push(format!("payload = {}", py_pairs(&pairs)));
                kwargs.push("data=payload".into());
            }
            if !files.is_empty() {
                let rows: String = files
                    .iter()
                    .map(|f| {
                        format!(
                            "    ({}, open({}, \"rb\")),\n",
                            py_str(&f.key),
                            py_str(&f.value)
                        )
                    })
                    .collect();
                blocks.push(format!("files = [\n{rows}]"));
                kwargs.push("files=files".into());
            }
        }
        RequestBody::Binary { path: Some(path) } => {
            blocks.push(format!("payload = open({}, \"rb\")", py_str(path)));
            kwargs.push("data=payload".into());
        }
    }

    if !headers.is_empty() {
        let pairs: Vec<(&str, &str)> = headers
            .iter()
            .map(|(k, v)| (k.as_str(), v.as_str()))
            .collect();
        let _ = writeln!(out, "headers = {}\n", py_dict(&pairs));
        kwargs.insert(0, "headers=headers".into());
    }
    for b in blocks {
        let _ = writeln!(out, "{b}\n");
    }
    if let Some((u, pw)) = p.basic {
        kwargs.push(format!("auth=({}, {})", py_str(u), py_str(pw)));
    }
    if let Some(secs) = timeout_secs(&p.settings) {
        kwargs.push(format!("timeout={secs}"));
    }
    if !p.settings.follow_redirects {
        kwargs.push("allow_redirects=False".into());
    }
    if !p.settings.verify_tls {
        kwargs.push("verify=False".into());
    }

    let _ = writeln!(
        out,
        "response = requests.request(\n    {},\n    url,",
        py_str(p.method.as_str())
    );
    for k in kwargs {
        let _ = writeln!(out, "    {k},");
    }
    out.push_str(")\n\nprint(response.text)\n");
    out
}

/// Double-quoted Python string literal.
fn py_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if c.is_control() => {
                let _ = write!(out, "\\u{:04x}", u32::from(c));
            }
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

/// A dict when keys are unique, else a list of tuples (keeps duplicates).
fn py_pairs(pairs: &[(&str, &str)]) -> String {
    let unique = pairs
        .iter()
        .enumerate()
        .all(|(i, (k, _))| pairs[..i].iter().all(|(o, _)| o != k));
    if unique {
        py_dict(pairs)
    } else {
        let rows: String = pairs
            .iter()
            .map(|(k, v)| format!("    ({}, {}),\n", py_str(k), py_str(v)))
            .collect();
        format!("[\n{rows}]")
    }
}

fn py_dict(pairs: &[(&str, &str)]) -> String {
    let rows: String = pairs
        .iter()
        .map(|(k, v)| format!("    {}: {},\n", py_str(k), py_str(v)))
        .collect();
    format!("{{\n{rows}}}")
}

/// JSON value → Python literal (`None`/`True`/`False`), 4-space indented.
fn py_value(v: &Value, depth: usize) -> String {
    let pad = "    ".repeat(depth + 1);
    let end = "    ".repeat(depth);
    match v {
        Value::Null => "None".into(),
        Value::Bool(true) => "True".into(),
        Value::Bool(false) => "False".into(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => py_str(s),
        Value::Array(items) if items.is_empty() => "[]".into(),
        Value::Array(items) => {
            let rows: String = items
                .iter()
                .map(|i| format!("{pad}{},\n", py_value(i, depth + 1)))
                .collect();
            format!("[\n{rows}{end}]")
        }
        Value::Object(map) if map.is_empty() => "{}".into(),
        Value::Object(map) => {
            let rows: String = map
                .iter()
                .map(|(k, i)| format!("{pad}{}: {},\n", py_str(k), py_value(i, depth + 1)))
                .collect();
            format!("{{\n{rows}{end}}}")
        }
    }
}

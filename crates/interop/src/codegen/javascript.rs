//! JavaScript `fetch` snippets (browser / Node 20+; file uploads use Node's
//! `openAsBlob`).

use super::{Prepared, file_name};
use ferrumapi_domain::{FormFieldKind, RawLanguage, RequestBody};
use std::fmt::Write as _;

pub(super) fn generate(p: &Prepared<'_>) -> String {
    let mut out = String::new();
    let uses_files = match p.body {
        RequestBody::Binary { path: Some(_) } => true,
        RequestBody::FormData { fields } => fields
            .iter()
            .any(|f| f.enabled && !f.key.is_empty() && f.kind == FormFieldKind::File),
        _ => false,
    };
    if uses_files {
        out.push_str("import { openAsBlob } from \"node:fs\";\n\n");
    }
    if !p.settings.verify_tls {
        out.push_str("// Note: fetch() cannot disable TLS certificate verification.\n");
    }
    let _ = writeln!(out, "const url = {};\n", js_str(&p.url));

    let mut headers = p.headers.clone();
    if let Some(b) = p.basic_header() {
        headers.push(("Authorization".into(), b));
    }
    if let Some(ct) = p.implied_content_type() {
        headers.push(("Content-Type".into(), ct.into()));
    }

    let body_option = body(p.body, &mut out);
    let _ = writeln!(out, "const response = await fetch(url, {{");
    let _ = writeln!(out, "  method: {},", js_str(p.method.as_str()));
    if !headers.is_empty() {
        out.push_str("  headers: {\n");
        for (k, v) in &headers {
            let _ = writeln!(out, "    {}: {},", js_str(k), js_str(v));
        }
        out.push_str("  },\n");
    }
    if let Some(b) = body_option {
        let _ = writeln!(out, "  {b},");
    }
    if !p.settings.follow_redirects {
        out.push_str("  redirect: \"manual\",\n");
    }
    if p.settings.timeout_ms > 0 {
        let _ = writeln!(
            out,
            "  signal: AbortSignal.timeout({}),",
            p.settings.timeout_ms
        );
    }
    out.push_str("});\n\nconsole.log(await response.text());\n");
    out
}

/// Writes any setup statements to `out`; returns the `body` option line.
fn body(body: &RequestBody, out: &mut String) -> Option<String> {
    match body {
        RequestBody::None | RequestBody::Binary { path: None } => None,
        RequestBody::Raw { text, .. } if text.is_empty() => None,
        RequestBody::Raw { language, text } => {
            let pretty = (*language == RawLanguage::Json)
                .then(|| serde_json::from_str::<serde_json::Value>(text).ok())
                .flatten()
                .and_then(|v| serde_json::to_string_pretty(&v).ok());
            Some(match pretty {
                Some(json) => format!("body: JSON.stringify({})", json.replace('\n', "\n  ")),
                None => format!("body: {}", js_str(text)),
            })
        }
        RequestBody::UrlEncoded { fields } => {
            out.push_str("const body = new URLSearchParams();\n");
            for f in fields.iter().filter(|f| f.is_active()) {
                let _ = writeln!(
                    out,
                    "body.append({}, {});",
                    js_str(&f.key),
                    js_str(&f.value)
                );
            }
            out.push('\n');
            Some("body".into())
        }
        RequestBody::FormData { fields } => {
            out.push_str("const body = new FormData();\n");
            for f in fields.iter().filter(|f| f.enabled && !f.key.is_empty()) {
                let _ = match f.kind {
                    FormFieldKind::Text => writeln!(
                        out,
                        "body.append({}, {});",
                        js_str(&f.key),
                        js_str(&f.value)
                    ),
                    FormFieldKind::File => writeln!(
                        out,
                        "body.append({}, await openAsBlob({}), {});",
                        js_str(&f.key),
                        js_str(&f.value),
                        js_str(file_name(&f.value))
                    ),
                };
            }
            out.push('\n');
            Some("body".into())
        }
        RequestBody::Binary { path: Some(path) } => {
            Some(format!("body: await openAsBlob({})", js_str(path)))
        }
    }
}

/// Double-quoted JS string literal (JSON string syntax is valid JS).
fn js_str(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

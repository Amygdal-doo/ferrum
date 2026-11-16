//! cURL (bash) snippets, one option per line.

use super::{Prepared, timeout_secs};
use ferrumapi_domain::{Auth, FormFieldKind, HttpMethod, RequestBody, RequestSettings};

/// Single-quotes `s` for a POSIX shell (`'` → `'\''`).
pub(crate) fn quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

pub(super) fn generate(p: &Prepared<'_>) -> String {
    let mut first = String::from("curl");
    if p.settings.follow_redirects {
        first.push_str(" --location");
    }
    if p.method == HttpMethod::Head {
        first.push_str(" --head");
    } else {
        first.push_str(" --request ");
        first.push_str(p.method.as_str());
    }
    let mut lines = vec![first, format!("--url {}", quote(&p.url))];
    if let Some((u, pw)) = p.basic {
        lines.push(format!("--user {}", quote(&format!("{u}:{pw}"))));
    }
    match p.signing {
        Some(Auth::Digest(a)) => {
            lines.push("--digest".into());
            lines.push(format!(
                "--user {}",
                quote(&format!("{}:{}", a.username, a.password))
            ));
        }
        Some(Auth::AwsSigV4(a)) => {
            lines.push(format!(
                "--aws-sigv4 {}",
                quote(&format!("aws:amz:{}:{}", a.region.trim(), a.service.trim()))
            ));
            lines.push(format!(
                "--user {}",
                quote(&format!("{}:{}", a.access_key_id, a.secret_access_key))
            ));
            if !a.session_token.is_empty() {
                lines.push(format!(
                    "--header {}",
                    quote(&format!("x-amz-security-token: {}", a.session_token))
                ));
            }
        }
        _ => {}
    }
    for (k, v) in &p.headers {
        if p.signing.is_some() && v == super::SIGNED_PLACEHOLDER {
            continue;
        }
        lines.push(format!("--header {}", quote(&format!("{k}: {v}"))));
    }
    if let Some(ct) = p.implied_content_type() {
        lines.push(format!(
            "--header {}",
            quote(&format!("Content-Type: {ct}"))
        ));
    }
    body(p.body, &mut lines);
    if p.settings.timeout_ms != RequestSettings::default().timeout_ms
        && let Some(secs) = timeout_secs(&p.settings)
    {
        lines.push(format!("--max-time {secs}"));
    }
    if !p.settings.verify_tls {
        lines.push("--insecure".into());
    }
    lines.join(" \\\n  ")
}

fn body(body: &RequestBody, lines: &mut Vec<String>) {
    match body {
        RequestBody::None | RequestBody::Binary { path: None } => {}
        RequestBody::Raw { text, .. } => {
            if !text.is_empty() {
                lines.push(format!("--data-raw {}", quote(text)));
            }
        }
        RequestBody::UrlEncoded { fields } => {
            for f in fields.iter().filter(|f| f.is_active()) {
                lines.push(format!(
                    "--data-urlencode {}",
                    quote(&format!("{}={}", f.key, f.value))
                ));
            }
        }
        RequestBody::FormData { fields } => {
            for f in fields.iter().filter(|f| f.enabled && !f.key.is_empty()) {
                let line = match f.kind {
                    FormFieldKind::File => {
                        format!("--form {}", quote(&format!("{}=@{}", f.key, f.value)))
                    }
                    // `@`, `<` and `;` are special in `--form` values.
                    FormFieldKind::Text
                        if f.value.starts_with(['@', '<', '"']) || f.value.contains(';') =>
                    {
                        format!("--form-string {}", quote(&format!("{}={}", f.key, f.value)))
                    }
                    FormFieldKind::Text => {
                        format!("--form {}", quote(&format!("{}={}", f.key, f.value)))
                    }
                };
                lines.push(line);
            }
        }
        RequestBody::Binary { path: Some(path) } => {
            lines.push(format!("--data-binary {}", quote(&format!("@{path}"))));
        }
    }
}

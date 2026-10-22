//! Code generation snapshots for every target.

mod common;

use common::*;
use ferrumapi_domain::{ApiKeyLocation, Auth, HttpMethod, KeyValue, RawLanguage, RequestBody};
use ferrumapi_interop::codegen::{CodeTarget, generate};

#[test]
fn labels() {
    let labels: Vec<_> = CodeTarget::ALL.iter().map(|t| t.label()).collect();
    assert_eq!(labels, ["cURL", "Python (requests)", "JavaScript (fetch)"]);
}

#[test]
fn curl_snapshot() {
    let mut r = json_post();
    r.settings.verify_tls = false;
    assert_eq!(
        generate(CodeTarget::Curl, &r),
        "curl --location --request POST \\\n  \
         --url 'https://api.dev/users?notify=true' \\\n  \
         --header 'Accept: application/json' \\\n  \
         --header 'Authorization: Bearer tok' \\\n  \
         --header 'Content-Type: application/json' \\\n  \
         --data-raw '{\"name\":\"O'\\''Brien\",\"admin\":false,\"tags\":[\"a\"],\"n\":null}' \\\n  \
         --insecure"
    );
}

#[test]
fn curl_form_basic_and_head() {
    let mut r = form_put();
    r.settings.follow_redirects = false;
    assert_eq!(
        generate(CodeTarget::Curl, &r),
        "curl --request PUT \\\n  --url 'https://api.dev/files' \\\n  --user 'ann:p@ss' \\\n  \
         --form-string 'title=Q3; final' \\\n  --form 'doc=@/tmp/report.pdf'"
    );
    let head = request(HttpMethod::Head, "https://x.dev");
    assert_eq!(
        generate(CodeTarget::Curl, &head),
        "curl --location --head \\\n  --url 'https://x.dev'"
    );
}

#[test]
fn api_key_in_query_and_header() {
    let mut r = request(HttpMethod::Get, "https://x.dev/a?b=1");
    r.auth = Auth::ApiKey {
        key: "api key".into(),
        value: "v&1".into(),
        location: ApiKeyLocation::Query,
    };
    assert!(generate(CodeTarget::Curl, &r).contains("--url 'https://x.dev/a?b=1&api+key=v%261'"));
    r.auth = Auth::ApiKey {
        key: "X-Key".into(),
        value: "k".into(),
        location: ApiKeyLocation::Header,
    };
    assert!(generate(CodeTarget::PythonRequests, &r).contains("\"X-Key\": \"k\","));
}

#[test]
fn python_snapshot() {
    assert_eq!(
        generate(CodeTarget::PythonRequests, &json_post()),
        r#"import requests

url = "https://api.dev/users?notify=true"

headers = {
    "Accept": "application/json",
    "Authorization": "Bearer tok",
}

payload = {
    "name": "O'Brien",
    "admin": False,
    "tags": [
        "a",
    ],
    "n": None,
}

response = requests.request(
    "POST",
    url,
    headers=headers,
    json=payload,
    timeout=30,
)

print(response.text)
"#
    );
}

#[test]
fn python_form_files_and_text_body() {
    let mut r = form_put();
    r.settings.verify_tls = false;
    r.settings.follow_redirects = false;
    let code = generate(CodeTarget::PythonRequests, &r);
    assert!(
        code.contains("payload = {\n    \"title\": \"Q3; final\",\n}"),
        "{code}"
    );
    assert!(
        code.contains("files = [\n    (\"doc\", open(\"/tmp/report.pdf\", \"rb\")),\n]"),
        "{code}"
    );
    assert!(
        code.contains("    data=payload,\n    files=files,\n    auth=(\"ann\", \"p@ss\"),\n"),
        "{code}"
    );
    assert!(
        code.contains("allow_redirects=False,\n    verify=False,\n)"),
        "{code}"
    );
    assert!(!code.contains("skip"));

    let mut r = request(HttpMethod::Post, "https://x.dev");
    r.body = RequestBody::Raw {
        language: RawLanguage::Xml,
        text: "<a>\"q\"</a>\n".into(),
    };
    let code = generate(CodeTarget::PythonRequests, &r);
    assert!(
        code.contains("\"Content-Type\": \"application/xml\""),
        "{code}"
    );
    assert!(code.contains("payload = \"<a>\\\"q\\\"</a>\\n\""), "{code}");
}

#[test]
fn javascript_snapshot() {
    assert_eq!(
        generate(CodeTarget::JavaScriptFetch, &json_post()),
        r#"const url = "https://api.dev/users?notify=true";

const response = await fetch(url, {
  method: "POST",
  headers: {
    "Accept": "application/json",
    "Authorization": "Bearer tok",
    "Content-Type": "application/json",
  },
  body: JSON.stringify({
    "name": "O'Brien",
    "admin": false,
    "tags": [
      "a"
    ],
    "n": null
  }),
  signal: AbortSignal.timeout(30000),
});

console.log(await response.text());
"#
    );
}

#[test]
fn javascript_form_and_urlencoded() {
    let code = generate(CodeTarget::JavaScriptFetch, &form_put());
    assert!(code.starts_with("import { openAsBlob } from \"node:fs\";"));
    assert!(
        code.contains(
            "body.append(\"doc\", await openAsBlob(\"/tmp/report.pdf\"), \"report.pdf\");"
        )
    );
    assert!(
        code.contains("\"Authorization\": \"Basic YW5uOnBAc3M=\","),
        "{code}"
    );
    let mut r = request(HttpMethod::Post, "https://x.dev");
    r.body = RequestBody::UrlEncoded {
        fields: vec![KeyValue::new("a", "1 2")],
    };
    r.settings.follow_redirects = false;
    let code = generate(CodeTarget::JavaScriptFetch, &r);
    assert!(
        code.contains("const body = new URLSearchParams();\nbody.append(\"a\", \"1 2\");"),
        "{code}"
    );
    assert!(
        code.contains("  body,\n  redirect: \"manual\",\n"),
        "{code}"
    );
}

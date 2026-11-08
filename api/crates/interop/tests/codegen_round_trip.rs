//! `curl::parse(generate(Curl, req))` preserves the request.

mod common;

use common::*;
use ferrumapi_domain::{Auth, HttpMethod, HttpRequest, KeyValue, RawLanguage, RequestBody};
use ferrumapi_interop::codegen::{CodeTarget, generate};
use ferrumapi_interop::curl;

fn assert_round_trip(r: &HttpRequest) {
    let cmd = generate(CodeTarget::Curl, r);
    let back = curl::parse(&cmd).unwrap_or_else(|e| panic!("{cmd}\n{e}"));
    assert_eq!(back.method, r.method, "{cmd}");
    assert_eq!(back.url, r.url, "{cmd}");
    assert_eq!(back.params, r.params, "{cmd}");
    assert_eq!(
        back.headers,
        r.headers
            .iter()
            .filter(|h| h.enabled)
            .cloned()
            .collect::<Vec<_>>(),
        "{cmd}"
    );
    assert_eq!(back.body, r.body, "{cmd}");
    assert_eq!(back.auth, r.auth, "{cmd}");
    assert_eq!(back.settings, r.settings, "{cmd}");
}

#[test]
fn curl_round_trips() {
    let mut json = json_post();
    json.headers.retain(|h| h.enabled);
    assert_round_trip(&json);

    let mut form = form_put();
    if let RequestBody::FormData { fields } = &mut form.body {
        fields.retain(|f| f.enabled);
    }
    assert_round_trip(&form);

    let mut text = request(HttpMethod::Patch, "https://x.dev/notes/1?draft&v={{v}}");
    text.body = RequestBody::Raw {
        language: RawLanguage::Text,
        text: "line 1\nit's \"quoted\" $HOME\n".into(),
    };
    text.headers = vec![KeyValue::new("X-Multi", "a, b; c=d")];
    text.settings.timeout_ms = 1500;
    text.settings.verify_tls = false;
    text.settings.follow_redirects = false;
    assert_round_trip(&text);

    let mut enc = request(HttpMethod::Post, "https://x.dev/login");
    enc.body = RequestBody::UrlEncoded {
        fields: vec![KeyValue::new("user", "a b&c"), KeyValue::new("pw", "x=y")],
    };
    enc.auth = Auth::Basic {
        username: "u".into(),
        password: "p'w".into(),
    };
    assert_round_trip(&enc);

    let mut bin = request(HttpMethod::Post, "https://x.dev/upload");
    bin.body = RequestBody::Binary {
        path: Some("/tmp/my file.bin".into()),
    };
    bin.headers = vec![KeyValue::new("Content-Type", "application/octet-stream")];
    assert_round_trip(&bin);

    let mut html = request(HttpMethod::Delete, "https://x.dev/page");
    html.body = RequestBody::Raw {
        language: RawLanguage::Html,
        text: "<p>hi</p>".into(),
    };
    assert_round_trip(&html);

    assert_round_trip(&request(HttpMethod::Options, "https://x.dev/"));
}

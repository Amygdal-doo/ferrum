//! cURL body inference: -d, --data-urlencode, -F, --data-binary, -G.

mod common;

use common::*;
use ferrumapi_domain::{FormFieldKind, HttpMethod, RawLanguage, RequestBody};

#[test]
fn json_post_with_explicit_method_and_line_continuations() {
    let cmd = "curl -X PUT \\\n  'https://api.dev/items/1' \\\n  -H \"Content-Type: application/json; charset=utf-8\" \\\n  -d '{\"name\": \"x\"}'";
    let r = parse(cmd);
    assert_eq!(r.method, HttpMethod::Put);
    assert_eq!(raw(&r), (RawLanguage::Json, "{\"name\": \"x\"}"));
    assert_eq!(
        header(&r, "content-type"),
        Some("application/json; charset=utf-8"),
        "non-default CT kept"
    );
}

#[test]
fn data_without_content_type() {
    let r = parse(r#"curl https://x.dev -d '{"a":1}'"#);
    assert_eq!(raw(&r), (RawLanguage::Json, r#"{"a":1}"#));
    let r = parse("curl https://x.dev -d 'a=1' -d 'b=hello%20world'");
    assert_eq!(r.method, HttpMethod::Post);
    assert_eq!(
        fields(&r),
        [("a".into(), "1".into()), ("b".into(), "hello world".into())]
    );
    let r = parse("curl https://x.dev --data 'just some text'");
    assert_eq!(raw(&r), (RawLanguage::Text, "just some text"));
    let r = parse("curl https://x.dev -H 'Content-Type: text/plain' --data-ascii 'a=1'");
    assert_eq!(raw(&r), (RawLanguage::Text, "a=1"));
    let r = parse("curl https://x.dev -H 'Content-Type: application/xml' --data-binary '<a/>'");
    assert_eq!(raw(&r), (RawLanguage::Xml, "<a/>"));
}

#[test]
fn form_content_type_and_urlencode() {
    let r = parse(
        "curl https://x.dev -H 'Content-Type: application/x-www-form-urlencoded' -d 'q=a+b&x'",
    );
    assert_eq!(
        fields(&r),
        [("q".into(), "a b".into()), ("x".into(), String::new())]
    );
    assert!(r.headers.is_empty());
    let r = parse("curl https://x.dev --data-urlencode 'msg=hi & bye' --data-urlencode 'raw'");
    assert_eq!(
        fields(&r),
        [
            ("msg".into(), "hi & bye".into()),
            ("raw".into(), String::new())
        ]
    );
}

#[test]
fn data_binary_file_and_upload() {
    let r = parse("curl https://x.dev/up --data-binary @./photo.jpg -H 'Content-Type: image/jpeg'");
    assert_eq!(
        r.body,
        RequestBody::Binary {
            path: Some("./photo.jpg".into())
        }
    );
    assert_eq!(header(&r, "content-type"), Some("image/jpeg"));
    let r = parse("curl -T /tmp/f.bin https://x.dev/put");
    assert_eq!(
        (r.method, r.body.clone()),
        (
            HttpMethod::Put,
            RequestBody::Binary {
                path: Some("/tmp/f.bin".into())
            }
        )
    );
    let r = parse("curl https://x.dev --data-raw '@not-a-file'");
    assert_eq!(raw(&r).1, "@not-a-file");
}

#[test]
fn multipart_form() {
    let r = parse(
        "curl -F 'name=Ann' -F 'avatar=@/home/ann/me.png;type=image/png' --form-string 'note=@literal' \
         -F 'meta=\"a;b\"' https://x.dev/profile -H 'Content-Type: multipart/form-data; boundary=xyz'",
    );
    assert_eq!(r.method, HttpMethod::Post);
    assert!(r.headers.is_empty(), "multipart boundary header dropped");
    let RequestBody::FormData { fields } = &r.body else {
        panic!("form")
    };
    let got: Vec<_> = fields
        .iter()
        .map(|f| (f.key.as_str(), f.value.as_str(), f.kind))
        .collect();
    assert_eq!(
        got,
        [
            ("name", "Ann", FormFieldKind::Text),
            ("avatar", "/home/ann/me.png", FormFieldKind::File),
            ("note", "@literal", FormFieldKind::Text),
            ("meta", "a;b", FormFieldKind::Text),
        ]
    );
}

#[test]
fn get_flag_moves_data_to_query() {
    let r = parse("curl -G https://x.dev/search?lang=en -d 'q=rust' --data-urlencode 'tag=a b'");
    assert_eq!(r.method, HttpMethod::Get);
    assert_eq!(r.url, "https://x.dev/search?lang=en&q=rust&tag=a+b");
    assert_eq!(r.params.len(), 3);
    assert_eq!(r.body, RequestBody::None);
}

#[test]
fn json_flag_and_double_quote_escapes() {
    let r = parse(r#"curl --json "{\"k\": \"\$HOME \\ x\"}" https://x.dev"#);
    assert_eq!(r.method, HttpMethod::Post);
    assert_eq!(raw(&r), (RawLanguage::Json, r#"{"k": "$HOME \ x"}"#));
    assert_eq!(header(&r, "Accept"), Some("application/json"));
}

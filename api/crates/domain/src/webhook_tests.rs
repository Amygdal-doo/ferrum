use super::*;

fn capture(method: &str, path: &str, query: &str, ct: &str, body: &[u8]) -> CapturedRequest {
    let mut headers = vec![
        ("Host".to_owned(), "127.0.0.1:9000".to_owned()),
        ("Content-Length".to_owned(), body.len().to_string()),
        ("X-Signature".to_owned(), "abc".to_owned()),
    ];
    if !ct.is_empty() {
        headers.push(("Content-Type".to_owned(), ct.to_owned()));
    }
    CapturedRequest {
        id: 0,
        received_at: Utc::now(),
        method: method.into(),
        path: path.into(),
        query: query.into(),
        headers,
        body: body.to_vec(),
        size: body.len(),
    }
}

#[test]
fn json_bodies_are_pretty_printed() {
    let c = capture(
        "POST",
        "/hook",
        "",
        "application/json",
        br#"{"a":1,"b":[true]}"#,
    );
    assert_eq!(
        c.display_body(),
        "{\n  \"a\": 1,\n  \"b\": [\n    true\n  ]\n}"
    );
    let text = capture("POST", "/hook", "", "text/plain", b"hello");
    assert_eq!(text.display_body(), "hello");
    let binary = capture("POST", "/hook", "", "", &[0xff, 0xfe, 0x00]);
    assert_eq!(binary.display_body(), "<3 bytes of binary data>");
    assert_eq!(capture("GET", "/", "", "", b"").display_body(), "");
}

#[test]
fn target_query_and_truncation() {
    let mut c = capture("GET", "/cb", "code=a%20b&state=1", "", b"");
    assert_eq!(c.target(), "/cb?code=a%20b&state=1");
    assert_eq!(
        c.query_pairs(),
        [("code".into(), "a b".into()), ("state".into(), "1".into())]
    );
    assert!(!c.truncated());
    c.size = 10;
    assert!(c.truncated());
    assert_eq!(c.header("x-signature"), Some("abc"));
}

#[test]
fn open_as_request_drops_hop_headers_and_keeps_body() {
    let c = capture("PUT", "/hook", "x=1", "application/json", br#"{"id":7}"#);
    let r = c.to_request("http://127.0.0.1:9000/");
    assert_eq!(r.method, HttpMethod::Put);
    assert_eq!(r.url, "http://127.0.0.1:9000/hook?x=1");
    assert_eq!(r.params, [KeyValue::new("x", "1")]);
    let names: Vec<&str> = r.headers.iter().map(|h| h.key.as_str()).collect();
    assert_eq!(names, ["X-Signature", "Content-Type"]);
    assert_eq!(
        r.body,
        RequestBody::Raw {
            language: RawLanguage::Json,
            text: "{\n  \"id\": 7\n}".into()
        }
    );

    let unknown = capture("PROPFIND", "/", "", "", b"");
    let r = unknown.to_request("http://h");
    assert_eq!(r.method, HttpMethod::Post);
    assert_eq!(r.body, RequestBody::None);

    let bad_json = capture("POST", "/", "", "application/json", b"{not json");
    let RequestBody::Raw { language, text } = bad_json.to_request("http://h").body else {
        panic!("raw body expected");
    };
    assert_eq!((language, text.as_str()), (RawLanguage::Text, "{not json"));

    let sniffed = capture("POST", "/", "", "", b"[1,2]");
    assert_eq!(sniffed.body_language(), RawLanguage::Json);
    let xml = capture("POST", "/", "", "text/xml; charset=utf-8", b"<a/>");
    assert_eq!(xml.body_language(), RawLanguage::Xml);
}

#[test]
fn log_drops_oldest_beyond_limit() {
    let mut log = CaptureLog::new(3);
    let ids: Vec<u64> = (0..5)
        .map(|i| log.push(capture("GET", &format!("/{i}"), "", "", b"")))
        .collect();
    assert_eq!(ids, [1, 2, 3, 4, 5]);
    assert_eq!(log.len(), 3);
    let paths: Vec<String> = log.newest_first().into_iter().map(|c| c.path).collect();
    assert_eq!(paths, ["/4", "/3", "/2"]);
    assert!(log.get(1).is_none());
    assert_eq!(log.get(5).map(|c| c.path.as_str()), Some("/4"));
    log.clear();
    assert!(log.is_empty());
    // Ids keep increasing after a clear.
    assert_eq!(log.push(capture("GET", "/", "", "", b"")), 6);
    assert_eq!(CaptureLog::default().limit, DEFAULT_CAPTURE_LIMIT);
}

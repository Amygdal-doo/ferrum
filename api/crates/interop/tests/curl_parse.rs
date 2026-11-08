//! cURL parsing: tokenizer, flags, auth, errors.

mod common;

use common::*;
use ferrumapi_domain::{Auth, HttpMethod, RawLanguage, RequestBody};
use ferrumapi_interop::{InteropError, curl};

#[test]
fn simple_get_with_defaults() {
    let r = parse("curl https://example.com/users?page=2");
    assert_eq!(r.method, HttpMethod::Get);
    assert_eq!(r.url, "https://example.com/users?page=2");
    assert_eq!(r.params[0].key, "page");
    assert_eq!(r.auth, Auth::None);
    assert!(
        !r.settings.follow_redirects,
        "curl does not follow redirects by default"
    );
    assert!(r.settings.verify_tls);
    assert_eq!(r.body, RequestBody::None);
}

#[test]
fn chrome_copy_as_curl() {
    let r = parse(CHROME);
    assert_eq!(r.method, HttpMethod::Post);
    assert_eq!(r.url, "https://api.example.com/graphql?op=Search");
    assert_eq!(
        r.auth,
        Auth::Bearer {
            token: "eyJhbGciOiJIUzI1NiJ9.e30.abc".into()
        }
    );
    assert_eq!(header(&r, "authorization"), None);
    assert_eq!(header(&r, "content-type"), None, "implied by the JSON body");
    assert_eq!(header(&r, "Cookie"), Some("session=abc123; theme=dark"));
    assert_eq!(
        header(&r, "sec-ch-ua").map(|v| v.contains("\"Not?A_Brand\";v=\"99\"")),
        Some(true)
    );
    assert_eq!(header(&r, "priority"), Some("u=1, i"));
    assert_eq!(r.headers.len(), 10);
    let (lang, text) = raw(&r);
    assert_eq!(lang, RawLanguage::Json);
    let v: serde_json::Value = serde_json::from_str(text).expect("json body");
    assert_eq!(v["variables"]["q"], "it's café");
    assert!(
        v["query"]
            .as_str()
            .is_some_and(|q| q.contains("{\n  search"))
    );
}

#[test]
fn basic_auth_and_authorization_headers() {
    let r = parse("curl -u alice:s3c:ret https://x.dev");
    assert_eq!(
        r.auth,
        Auth::Basic {
            username: "alice".into(),
            password: "s3c:ret".into()
        }
    );
    let r = parse("curl --user bob https://x.dev");
    assert_eq!(
        r.auth,
        Auth::Basic {
            username: "bob".into(),
            password: String::new()
        }
    );
    let r = parse("curl -H 'Authorization: Basic YWxpY2U6cHc=' https://x.dev");
    assert_eq!(
        r.auth,
        Auth::Basic {
            username: "alice".into(),
            password: "pw".into()
        }
    );
    assert!(r.headers.is_empty());
    let r = parse("curl -H 'Authorization: Token abc' https://x.dev");
    assert_eq!(
        (r.auth.clone(), header(&r, "Authorization")),
        (Auth::None, Some("Token abc"))
    );
    let r = parse("curl --oauth2-bearer tok https://x.dev");
    assert_eq!(
        r.auth,
        Auth::Bearer {
            token: "tok".into()
        }
    );
}

#[test]
fn combined_and_attached_short_flags() {
    let r = parse("curl -sSLk -XDELETE -H'X-Req: 1' -m 2.5 https://x.dev/a");
    assert_eq!(r.method, HttpMethod::Delete);
    assert!(r.settings.follow_redirects);
    assert!(!r.settings.verify_tls);
    assert_eq!(r.settings.timeout_ms, 2500);
    assert_eq!(header(&r, "X-Req"), Some("1"));
    let r = parse("curl -sLXPOST https://x.dev/b -dfoo=bar");
    assert_eq!(
        (r.method, fields(&r)),
        (HttpMethod::Post, vec![("foo".into(), "bar".into())])
    );
}

#[test]
fn misc_flags_become_headers_or_settings() {
    let r = parse(
        "curl --url https://x.dev -I -A 'MyAgent/1.0' -e https://ref.dev --cookie 'a=1' \
         --compressed --max-time 10 --location --insecure -v",
    );
    assert_eq!(r.method, HttpMethod::Head);
    assert_eq!(header(&r, "User-Agent"), Some("MyAgent/1.0"));
    assert_eq!(header(&r, "Referer"), Some("https://ref.dev"));
    assert_eq!(header(&r, "Cookie"), Some("a=1"));
    assert_eq!(r.settings.timeout_ms, 10_000);
    assert!(r.settings.follow_redirects && !r.settings.verify_tls);
    let r = parse("curl -b cookies.txt https://x.dev");
    assert!(r.headers.is_empty(), "cookie file is ignored");
}

#[test]
fn unknown_flags_with_values_are_skipped() {
    let r = parse(
        "curl -o out.json --connect-timeout 5 -w '%{http_code}' --retry 3 --proxy http://p:8080 \
         --http2 --fail-with-body -x http://p:1 https://x.dev/real",
    );
    assert_eq!(r.url, "https://x.dev/real");
}

#[test]
fn windows_caret_continuation_and_scheme_default() {
    let r = parse("curl example.com/a ^\r\n  -H \"X-A: 1\"");
    assert_eq!(r.url, "http://example.com/a");
    assert_eq!(header(&r, "X-A"), Some("1"));
}

#[test]
fn ansi_c_escapes() {
    let words = curl::tokenize(r"curl $'a\tb\x41\101é\n' $'it\'s'").expect("tokenize");
    assert_eq!(words, ["curl", "a\tbAAé\n", "it's"]);
    let words = curl::tokenize("curl '' x\\ y").expect("tokenize");
    assert_eq!(words, ["curl", "", "x y"]);
}

#[test]
fn errors() {
    assert!(matches!(curl::parse(""), Err(InteropError::Parse(_))));
    assert!(matches!(
        curl::parse("wget https://x.dev"),
        Err(InteropError::Parse(_))
    ));
    assert!(
        matches!(curl::parse("curl -H 'A: b'"), Err(InteropError::Parse(m)) if m.contains("URL"))
    );
    assert!(
        matches!(curl::parse("curl 'https://x.dev"), Err(InteropError::Parse(m)) if m.contains("unterminated"))
    );
    assert!(matches!(
        curl::parse("curl https://x.dev -H"),
        Err(InteropError::Parse(_))
    ));
    assert!(matches!(
        curl::parse("curl -X BREW https://x.dev"),
        Err(InteropError::Parse(_))
    ));
    assert!(matches!(
        curl::parse("curl -m soon https://x.dev"),
        Err(InteropError::Parse(_))
    ));
}

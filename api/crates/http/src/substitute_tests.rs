use super::*;
use ferrumapi_domain::{ApiKeyLocation, FormField, FormFieldKind, HttpMethod, RawLanguage};

fn vars() -> VariableSet {
    let mut v = VariableSet::new();
    v.insert("host", "example.test");
    v.insert("k", "KEY");
    v.insert("v", "VAL");
    v
}

fn kv(k: &str, v: &str, enabled: bool) -> KeyValue {
    KeyValue {
        enabled,
        ..KeyValue::new(k, v)
    }
}

#[test]
fn substitutes_url_params_headers() {
    let mut req = HttpRequest::new(HttpMethod::Get, "https://{{host}}/x?{{k}}={{v}}");
    req.params = vec![kv("{{k}}", "{{v}}", true)];
    req.headers = vec![kv("X-{{k}}", "{{v}}-{{missing}}", false)];
    let out = substitute(&req, &vars());
    assert_eq!(out.url, "https://example.test/x?KEY=VAL");
    assert_eq!(out.params[0].key, "KEY");
    assert_eq!(out.params[0].value, "VAL");
    assert_eq!(out.headers[0].key, "X-KEY");
    assert_eq!(out.headers[0].value, "VAL-{{missing}}");
    assert!(!out.headers[0].enabled, "enabled flag preserved");
}

#[test]
fn substitutes_bodies() {
    let v = vars();
    let mut req = HttpRequest::new(HttpMethod::Post, "");
    req.body = RequestBody::Raw {
        language: RawLanguage::Json,
        text: r#"{"a":"{{v}}"}"#.into(),
    };
    assert_eq!(
        substitute(&req, &v).body,
        RequestBody::Raw {
            language: RawLanguage::Json,
            text: r#"{"a":"VAL"}"#.into()
        }
    );
    req.body = RequestBody::UrlEncoded {
        fields: vec![kv("{{k}}", "{{v}}", true)],
    };
    assert_eq!(
        substitute(&req, &v).body,
        RequestBody::UrlEncoded {
            fields: vec![kv("KEY", "VAL", true)]
        }
    );
    let file = FormField {
        key: "{{k}}".into(),
        value: "/tmp/{{v}}.txt".into(),
        kind: FormFieldKind::File,
        enabled: true,
    };
    req.body = RequestBody::FormData { fields: vec![file] };
    let RequestBody::FormData { fields } = substitute(&req, &v).body else {
        panic!("expected form data");
    };
    assert_eq!(fields[0].key, "KEY");
    assert_eq!(fields[0].value, "/tmp/VAL.txt");
    assert_eq!(fields[0].kind, FormFieldKind::File);
    req.body = RequestBody::Binary {
        path: Some("/data/{{v}}".into()),
    };
    assert_eq!(
        substitute(&req, &v).body,
        RequestBody::Binary {
            path: Some("/data/VAL".into())
        }
    );
}

#[test]
fn substitutes_auth() {
    let v = vars();
    let mut req = HttpRequest::new(HttpMethod::Post, "");
    req.auth = Auth::Basic {
        username: "{{k}}".into(),
        password: "{{v}}".into(),
    };
    assert_eq!(
        substitute(&req, &v).auth,
        Auth::Basic {
            username: "KEY".into(),
            password: "VAL".into()
        }
    );
    req.auth = Auth::Bearer {
        token: "{{v}}".into(),
    };
    assert_eq!(
        substitute(&req, &v).auth,
        Auth::Bearer {
            token: "VAL".into()
        }
    );
    req.auth = Auth::ApiKey {
        key: "{{k}}".into(),
        value: "{{v}}".into(),
        location: ApiKeyLocation::Query,
    };
    assert_eq!(
        substitute(&req, &v).auth,
        Auth::ApiKey {
            key: "KEY".into(),
            value: "VAL".into(),
            location: ApiKeyLocation::Query
        }
    );
}

#[test]
fn unresolved_covers_all_fields_enabled_only_dedup() {
    let mut req = HttpRequest::new(HttpMethod::Post, "{{base}}/x");
    req.params = vec![kv("p", "{{base}}", true), kv("q", "{{off_param}}", false)];
    req.headers = vec![kv("{{hk}}", "{{hv}}", true), kv("x", "{{off_hdr}}", false)];
    req.body = RequestBody::FormData {
        fields: vec![
            FormField {
                key: "f".into(),
                value: "{{fv}}".into(),
                ..FormField::default()
            },
            FormField {
                key: "g".into(),
                value: "{{off_form}}".into(),
                enabled: false,
                ..FormField::default()
            },
        ],
    };
    req.auth = Auth::Bearer {
        token: "{{tok}}".into(),
    };
    let names = unresolved_variables(&req, &vars());
    assert_eq!(names, ["base", "hk", "hv", "fv", "tok"]);
}

#[test]
fn unresolved_raw_urlencoded_binary_apikey() {
    let mut req = HttpRequest::new(HttpMethod::Post, "http://{{host}}");
    req.body = RequestBody::Raw {
        language: RawLanguage::Text,
        text: "{{a}} {{a}}".into(),
    };
    req.auth = Auth::ApiKey {
        key: "{{ak}}".into(),
        value: "{{k}}".into(),
        location: ApiKeyLocation::Header,
    };
    assert_eq!(unresolved_variables(&req, &vars()), ["a", "ak"]);
    req.body = RequestBody::UrlEncoded {
        fields: vec![kv("{{u}}", "x", true), kv("{{u_off}}", "x", false)],
    };
    assert_eq!(unresolved_variables(&req, &vars()), ["u", "ak"]);
    req.body = RequestBody::Binary {
        path: Some("{{bin}}".into()),
    };
    req.auth = Auth::Inherit;
    assert_eq!(unresolved_variables(&req, &vars()), ["bin"]);
}

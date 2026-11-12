use super::*;
use crate::body::{RawLanguage, RequestBody};
use crate::request::HttpMethod;
use serde_json::{Value, json};

fn raw_json(r: &HttpRequest) -> Value {
    match &r.body {
        RequestBody::Raw {
            language: RawLanguage::Json,
            text,
        } => serde_json::from_str(text).expect("json body"),
        other => panic!("expected a JSON body, got {other:?}"),
    }
}

fn header<'a>(r: &'a HttpRequest, name: &str) -> Option<&'a str> {
    r.headers
        .iter()
        .find(|h| h.key.eq_ignore_ascii_case(name))
        .map(|h| h.value.as_str())
}

#[test]
fn old_records_load_as_rest_and_specs_are_not_written_when_unused() {
    let r: HttpRequest = serde_json::from_str(r#"{"method":"GET","url":"x"}"#).expect("load");
    assert_eq!(r.kind, RequestKind::Rest);
    let text = serde_json::to_string(&r).expect("save");
    assert!(!text.contains("specs"), "{text}");
    let gql = HttpRequest::of_kind(RequestKind::Graphql);
    let back: HttpRequest =
        serde_json::from_str(&serde_json::to_string(&gql).expect("save")).expect("load");
    assert_eq!(back, gql);
    assert!(
        serde_json::to_string(&gql)
            .expect("save")
            .contains(r#""kind":"graphql""#)
    );
}

#[test]
fn rest_is_unchanged_and_streaming_kinds_are_rejected() {
    let r = HttpRequest::new(HttpMethod::Put, "http://x/a");
    assert_eq!(lower(&r).expect("rest"), r);
    for kind in [RequestKind::Grpc, RequestKind::WebSocket] {
        assert!(matches!(
            lower(&HttpRequest::of_kind(kind)),
            Err(ExecError::InvalidRequest(_))
        ));
    }
}

#[test]
fn graphql_post_sends_query_variables_and_operation_name() {
    let mut r = HttpRequest::of_kind(RequestKind::Graphql);
    r.url = "http://x/graphql".into();
    r.specs.graphql.query = "query Q($id: ID!) { user(id: $id) { name } }".into();
    r.specs.graphql.variables = r#"{"id": "7"}"#.into();
    r.specs.graphql.operation_name = "Q".into();
    let out = lower(&r).expect("lower");
    assert_eq!(out.method, HttpMethod::Post);
    assert_eq!(out.kind, RequestKind::Rest);
    assert_eq!(
        raw_json(&out),
        json!({"query": r.specs.graphql.query, "variables": {"id": "7"}, "operationName": "Q"})
    );
    assert!(header(&out, "accept").is_some_and(|a| a.contains("json")));

    r.specs.graphql.variables = "  ".into();
    r.specs.graphql.operation_name.clear();
    assert_eq!(
        raw_json(&lower(&r).expect("lower")),
        json!({"query": r.specs.graphql.query})
    );

    r.specs.graphql.variables = "[1]".into();
    assert!(lower(&r).is_err());
    r.specs.graphql.variables = "{oops".into();
    assert!(lower(&r).is_err());
    r.specs.graphql.query = " ".into();
    assert!(lower(&r).is_err());
}

#[test]
fn graphql_get_encodes_fields_into_the_query_string() {
    let mut r = HttpRequest::of_kind(RequestKind::Graphql);
    r.url = "http://x/graphql?app=1".into();
    r.specs.graphql.query = "{ a }".into();
    r.specs.graphql.variables = r#"{"q":"a&b"}"#.into();
    r.specs.graphql.use_get = true;
    let out = lower(&r).expect("lower");
    assert_eq!(out.method, HttpMethod::Get);
    assert_eq!(out.body, RequestBody::None);
    assert_eq!(
        out.url,
        "http://x/graphql?app=1&query=%7B%20a%20%7D&variables=%7B%22q%22%3A%22a%26b%22%7D"
    );
}

#[test]
fn soap_versions_set_content_type_and_action() {
    let mut r = HttpRequest::of_kind(RequestKind::Soap);
    r.url = "http://x/svc".into();
    r.method = HttpMethod::Get;
    r.specs.soap.action = "urn:Add".into();
    let out = lower(&r).expect("lower");
    assert_eq!(out.method, HttpMethod::Post);
    assert_eq!(
        header(&out, "content-type"),
        Some("text/xml; charset=utf-8")
    );
    assert_eq!(header(&out, "soapaction"), Some("\"urn:Add\""));
    assert!(
        matches!(&out.body, RequestBody::Raw { language: RawLanguage::Xml, text } if text.contains("soap:Envelope"))
    );

    r.specs.soap.version = SoapVersion::V12;
    let out = lower(&r).expect("lower");
    assert_eq!(
        header(&out, "content-type"),
        Some("application/soap+xml; charset=utf-8; action=\"urn:Add\"")
    );
    assert_eq!(header(&out, "soapaction"), None);

    // A user Content-Type wins.
    r.headers.push(KeyValue::new("Content-Type", "text/xml"));
    let out = lower(&r).expect("lower");
    assert_eq!(header(&out, "content-type"), Some("text/xml"));
    assert!(soap::envelope(SoapVersion::V12, "<a/>").contains("2003/05/soap-envelope"));
}

#[test]
fn odata_builds_the_url_from_the_query_options() {
    let mut r = HttpRequest::of_kind(RequestKind::Odata);
    r.url = "https://svc/odata/".into();
    let o = &mut r.specs.odata;
    o.entity_set = "People".into();
    o.filter = "FirstName eq 'Scott' and Age gt 3".into();
    o.select = "FirstName,LastName".into();
    o.expand = "Friends($top=2)".into();
    o.orderby = "LastName desc".into();
    o.top = "5".into();
    o.skip = " ".into();
    o.count = true;
    let out = lower(&r).expect("lower");
    assert_eq!(
        out.url,
        "https://svc/odata/People?$filter=FirstName%20eq%20'Scott'%20and%20Age%20gt%203\
         &$select=FirstName,LastName&$expand=Friends($top%3D2)&$orderby=LastName%20desc\
         &$top=5&$count=true"
    );
    assert_eq!(header(&out, "accept"), Some("application/json"));
    assert_eq!(
        odata::metadata_url("https://svc/odata/?x=1"),
        "https://svc/odata/$metadata"
    );
    assert_eq!(
        odata::build_url("http://s/r?key=1", &ODataSpec::default()),
        "http://s/r?key=1"
    );
}

#[test]
fn trpc_queries_mutations_and_batches() {
    let mut r = HttpRequest::of_kind(RequestKind::Trpc);
    r.url = "http://x/trpc/".into();
    r.specs.trpc.procedure = "user.byId".into();
    r.specs.trpc.input = r#"{"id": 1}"#.into();
    let out = lower(&r).expect("lower");
    assert_eq!(out.method, HttpMethod::Get);
    assert_eq!(out.url, "http://x/trpc/user.byId?input=%7B%22id%22%3A1%7D");

    r.specs.trpc.batch = true;
    let out = lower(&r).expect("lower");
    assert_eq!(
        out.url,
        "http://x/trpc/user.byId?batch=1&input=%7B%220%22%3A%7B%22id%22%3A1%7D%7D"
    );

    r.specs.trpc.procedure_type = TrpcType::Mutation;
    let out = lower(&r).expect("lower");
    assert_eq!(out.method, HttpMethod::Post);
    assert_eq!(out.url, "http://x/trpc/user.byId?batch=1");
    assert_eq!(raw_json(&out), json!({"0": {"id": 1}}));

    r.specs.trpc.batch = false;
    r.specs.trpc.input.clear();
    let out = lower(&r).expect("lower");
    assert_eq!(out.body, RequestBody::None);
    assert_eq!(out.url, "http://x/trpc/user.byId");

    r.specs.trpc.input = "nope".into();
    assert!(lower(&r).is_err());
    r.specs.trpc.procedure.clear();
    assert!(lower(&r).is_err());
}

#[test]
fn trpc_responses_unwrap_result_data() {
    use trpc::{TrpcOutcome, unwrap_response};
    assert_eq!(
        unwrap_response(br#"{"result":{"data":{"id":1}}}"#),
        Some(TrpcOutcome::Data(json!({"id": 1})))
    );
    assert_eq!(
        unwrap_response(br#"[{"result":{"data":{"json":[1,2]}}}]"#),
        Some(TrpcOutcome::Data(json!([1, 2])))
    );
    assert_eq!(
        unwrap_response(br#"{"error":{"message":"bad","code":-32600}}"#),
        Some(TrpcOutcome::Error(json!({"message":"bad","code":-32600})))
    );
    assert_eq!(unwrap_response(b"{\"x\":1}"), None);
    assert_eq!(unwrap_response(b"not json"), None);
}

#[test]
fn query_helpers() {
    assert_eq!(encode_component("a b&c/é"), "a%20b%26c%2F%C3%A9");
    let pairs = [("a".to_owned(), "1".to_owned())];
    assert_eq!(append_query("http://x#f", &pairs), "http://x?a=1#f");
    assert_eq!(append_query("http://x?", &pairs), "http://x?a=1");
    assert_eq!(append_query("http://x?b=2", &pairs), "http://x?b=2&a=1");
    assert_eq!(append_query("http://x", &[]), "http://x");
}

#[test]
fn grpc_and_websocket_helpers() {
    use grpc::{endpoint, split_method};
    assert_eq!(
        split_method("pkg.Greeter/SayHello"),
        Some(("pkg.Greeter", "SayHello"))
    );
    assert_eq!(
        split_method("/pkg.Greeter.SayHello"),
        Some(("pkg.Greeter", "SayHello"))
    );
    assert_eq!(split_method("nothing"), None);
    assert_eq!(
        endpoint("localhost:50051", false),
        ("http://localhost:50051".into(), false)
    );
    assert_eq!(
        endpoint("grpcs://api:443/", false),
        ("https://api:443".into(), true)
    );
    assert_eq!(endpoint("grpc://h:1", true), ("https://h:1".into(), true));

    let spec = WsSpec {
        subprotocols: "graphql-ws, , chat ".into(),
        ..WsSpec::default()
    };
    assert_eq!(spec.subprotocol_list(), ["graphql-ws", "chat"]);
    assert!(websocket::validate_payload(WsPayloadKind::Json, "{\"a\":1}").is_ok());
    assert!(websocket::validate_payload(WsPayloadKind::Json, "{").is_err());
    let mut log = websocket::WsLog::default();
    for _ in 0..websocket::WS_LOG_CAP + 5 {
        log.push(websocket::WsLogEntry::event(websocket::WsFrame::Info, "x"));
    }
    assert_eq!(log.entries.len(), websocket::WS_LOG_CAP);
}

#[test]
fn labels_use_the_procedure_or_method() {
    let mut r = HttpRequest::of_kind(RequestKind::Grpc);
    r.url = "grpc://localhost:50051".into();
    assert_eq!(r.label(), "localhost:50051");
    r.specs.grpc.method = "pkg.S/M".into();
    assert_eq!(r.label(), "pkg.S/M");
}

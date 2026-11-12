//! Postman export / import and code generation for the non-REST kinds.

use ferrumapi_domain::protocol::grpc::GrpcSchemaSource;
use ferrumapi_domain::{Auth, Collection, HttpRequest, Item, KeyValue, RequestKind, SavedRequest};
use ferrumapi_interop::codegen::{CodeTarget, generate};
use ferrumapi_interop::postman;
use serde_json::Value;

fn mixed() -> Collection {
    let mut c = Collection::new("Mixed");
    let mut gql = HttpRequest::of_kind(RequestKind::Graphql);
    gql.url = "{{baseUrl}}/graphql".into();
    gql.specs.graphql.query = "{ me { id } }".into();
    gql.specs.graphql.variables = "{\"a\": 1}".into();
    let mut trpc = HttpRequest::of_kind(RequestKind::Trpc);
    trpc.url = "{{baseUrl}}/trpc".into();
    trpc.specs.trpc.procedure = "post.list".into();
    trpc.specs.trpc.input = "{\"limit\": {{limit}}}".into();
    let mut ws = HttpRequest::of_kind(RequestKind::WebSocket);
    ws.url = "ws://localhost/ws".into();
    let grpc = HttpRequest::of_kind(RequestKind::Grpc);
    for (name, r) in [("gql", gql), ("trpc", trpc), ("socket", ws), ("rpc", grpc)] {
        c.items.push(Item::Request(SavedRequest::new(name, r)));
    }
    c
}

#[test]
fn export_keeps_graphql_lowers_http_kinds_and_annotates_skipped_ones() {
    let json = postman::export_collection(&mixed()).expect("export");
    let v: Value = serde_json::from_str(&json).expect("json");
    let items = v["item"].as_array().expect("items");
    assert_eq!(items.len(), 2, "gRPC and WebSocket are skipped");
    let gql = &items[0]["request"];
    assert_eq!(gql["method"], "POST");
    assert_eq!(gql["body"]["mode"], "graphql");
    assert_eq!(gql["body"]["graphql"]["query"], "{ me { id } }");
    // tRPC is exported as the GET it sends; variables survive encoding.
    let trpc = &items[1]["request"];
    assert_eq!(trpc["method"], "GET");
    assert_eq!(
        trpc["url"]["raw"],
        "{{baseUrl}}/trpc/post.list?input=%7B%22limit%22%3A{{limit}}%7D"
    );
    assert!(
        trpc["description"]
            .as_str()
            .is_some_and(|d| d.contains("tRPC"))
    );
    let desc = v["info"]["description"].as_str().expect("description");
    assert!(
        desc.contains("2 gRPC / WebSocket") && desc.contains("socket, rpc"),
        "{desc}"
    );

    // GraphQL round-trips as a GraphQL request.
    let back = postman::import_collection(&json).expect("import");
    let Item::Request(r) = &back.items[0] else {
        panic!("request")
    };
    assert_eq!(r.request.kind, RequestKind::Graphql);
    assert_eq!(r.request.specs.graphql.query, "{ me { id } }");
    assert_eq!(r.request.specs.graphql.variables, "{\"a\": 1}");
}

#[test]
fn codegen_renders_lowered_http_and_streaming_commands() {
    let mut gql = HttpRequest::of_kind(RequestKind::Graphql);
    gql.url = "http://x/graphql".into();
    gql.specs.graphql.query = "{ a }".into();
    let curl = generate(CodeTarget::Curl, &gql);
    assert!(curl.contains("POST") || curl.contains("--data"), "{curl}");
    assert!(curl.contains(r#"{"query":"{ a }"}"#), "{curl}");
    let js = generate(CodeTarget::JavaScriptFetch, &gql);
    assert!(js.contains("fetch(") && js.contains("query"), "{js}");

    let mut rpc = HttpRequest::of_kind(RequestKind::Grpc);
    rpc.url = "localhost:50051".into();
    rpc.specs.grpc.method = "pkg.Greeter/SayHello".into();
    rpc.specs.grpc.message = "{\"name\":\"a\"}".into();
    rpc.headers.push(KeyValue::new("x-team", "core"));
    rpc.auth = Auth::Bearer { token: "t".into() };
    let cmd = generate(CodeTarget::Curl, &rpc);
    assert!(cmd.starts_with("grpcurl"), "{cmd}");
    assert!(cmd.contains("-plaintext"));
    assert!(cmd.contains("-H 'x-team: core'"));
    assert!(cmd.contains("-H 'Authorization: Bearer t'"));
    assert!(cmd.contains("'localhost:50051'") && cmd.contains("'pkg.Greeter/SayHello'"));
    rpc.url = "grpcs://api:443".into();
    rpc.specs.grpc.source = GrpcSchemaSource::ProtoFile;
    rpc.specs.grpc.proto_file = "/p/a.proto".into();
    let cmd = generate(CodeTarget::Curl, &rpc);
    assert!(
        !cmd.contains("-plaintext") && cmd.contains("-proto '/p/a.proto'"),
        "{cmd}"
    );

    let mut ws = HttpRequest::of_kind(RequestKind::WebSocket);
    ws.url = "wss://h/s".into();
    ws.specs.websocket.subprotocols = "chat".into();
    let cmd = generate(CodeTarget::PythonRequests, &ws);
    assert!(
        cmd.starts_with("websocat")
            && cmd.contains("--protocol 'chat'")
            && cmd.contains("'wss://h/s'"),
        "{cmd}"
    );

    let mut bad = HttpRequest::of_kind(RequestKind::Trpc);
    bad.url = "http://x".into();
    assert!(generate(CodeTarget::Curl, &bad).starts_with("# invalid request"));
}

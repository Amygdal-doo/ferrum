//! gRPC against a local tonic server with server reflection, plus .proto
//! file compilation. The test service is dynamic (prost-reflect), compiled
//! from `PROTO` at runtime, so there is no build script.

use ferrumapi_domain::protocol::grpc::GrpcSchemaSource;
use ferrumapi_domain::{Auth, ExecError, HttpRequest, KeyValue, RequestKind};
use ferrumapi_grpc::codec::DynamicCodec;
use ferrumapi_grpc::{DescriptorPool, invoke, load_schema, methods};
use futures::Stream;
use prost_reflect::{DynamicMessage, Value};
use std::convert::Infallible;
use std::pin::Pin;
use std::task::{Context, Poll};
use tokio_util::sync::CancellationToken;
use tonic::body::Body;
use tonic::server::NamedService;
use tonic::{Request, Response, Status};

const PROTO: &str = r#"syntax = "proto3";
package test;
service Greeter {
  rpc SayHello (HelloRequest) returns (HelloReply);
  rpc Count (CountRequest) returns (stream CountReply);
  rpc Fail (HelloRequest) returns (HelloReply);
  rpc Upload (stream HelloRequest) returns (HelloReply);
}
message HelloRequest { string name = 1; int32 times = 2; }
message HelloReply { string message = 1; string auth = 2; string team = 3; }
message CountRequest { int32 to = 1; }
message CountReply { int32 n = 1; }
"#;

fn proto_dir() -> tempfile::TempDir {
    let dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dir.path().join("greeter.proto"), PROTO).expect("write proto");
    dir
}

fn pool(dir: &tempfile::TempDir) -> DescriptorPool {
    let file = dir.path().join("greeter.proto");
    ferrumapi_grpc::compile_proto(file.to_str().expect("utf8"), &[]).expect("compile")
}

#[derive(Clone)]
struct Greeter(DescriptorPool);

impl NamedService for Greeter {
    const NAME: &'static str = "test.Greeter";
}

type BoxFut = Pin<Box<dyn Future<Output = Result<http::Response<Body>, Infallible>> + Send>>;
type Replies = Pin<Box<dyn Stream<Item = Result<DynamicMessage, Status>> + Send>>;

fn text(m: &DynamicMessage, field: &str) -> String {
    m.get_field_by_name(field)
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn meta(req: &Request<DynamicMessage>, key: &str) -> String {
    req.metadata()
        .get(key)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

impl tower::Service<http::Request<Body>> for Greeter {
    type Response = http::Response<Body>;
    type Error = Infallible;
    type Future = BoxFut;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), Infallible>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: http::Request<Body>) -> BoxFut {
        let pool = self.0.clone();
        let msg = |name: &str| pool.get_message_by_name(name).expect("message");
        let (input, output) = match req.uri().path() {
            "/test.Greeter/Count" => (msg("test.CountRequest"), msg("test.CountReply")),
            _ => (msg("test.HelloRequest"), msg("test.HelloReply")),
        };
        Box::pin(async move {
            let mut grpc = tonic::server::Grpc::new(DynamicCodec::new(input));
            let resp = match req.uri().path() {
                "/test.Greeter/SayHello" => {
                    let svc = tower::service_fn(move |r: Request<DynamicMessage>| {
                        let mut reply = DynamicMessage::new(output.clone());
                        let name = text(r.get_ref(), "name");
                        reply
                            .set_field_by_name("message", Value::String(format!("Hello, {name}!")));
                        reply.set_field_by_name("auth", Value::String(meta(&r, "authorization")));
                        reply.set_field_by_name("team", Value::String(meta(&r, "x-team")));
                        let mut resp = Response::new(reply);
                        resp.metadata_mut()
                            .insert("x-served-by", "test".parse().expect("meta"));
                        async move { Ok::<_, Status>(resp) }
                    });
                    grpc.unary(svc, req).await
                }
                "/test.Greeter/Fail" => {
                    let svc = tower::service_fn(|_: Request<DynamicMessage>| async {
                        Err::<Response<DynamicMessage>, _>(Status::not_found("no such user"))
                    });
                    grpc.unary(svc, req).await
                }
                "/test.Greeter/Count" => {
                    let svc = tower::service_fn(move |r: Request<DynamicMessage>| {
                        let to = r
                            .get_ref()
                            .get_field_by_name("to")
                            .and_then(|v| v.as_i32())
                            .unwrap_or(0);
                        let replies: Vec<Result<DynamicMessage, Status>> = (1..=to)
                            .map(|n| {
                                let mut m = DynamicMessage::new(output.clone());
                                m.set_field_by_name("n", Value::I32(n));
                                Ok(m)
                            })
                            .collect();
                        let stream: Replies = Box::pin(futures::stream::iter(replies));
                        async move { Ok::<_, Status>(Response::new(stream)) }
                    });
                    grpc.server_streaming(svc, req).await
                }
                _ => Status::unimplemented("unknown method").into_http(),
            };
            Ok(resp)
        })
    }
}

/// Starts the server (with reflection) and returns `host:port`.
async fn server(pool: DescriptorPool) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    let fds = pool.encode_to_vec();
    let reflection = tonic_reflection::server::Builder::configure()
        .register_encoded_file_descriptor_set(Box::leak(fds.into_boxed_slice()))
        .build_v1()
        .expect("reflection");
    tokio::spawn(async move {
        tonic::transport::Server::builder()
            .add_service(Greeter(pool))
            .add_service(reflection)
            .serve_with_incoming(tokio_stream::wrappers::TcpListenerStream::new(listener))
            .await
            .expect("serve");
    });
    format!("127.0.0.1:{}", addr.port())
}

fn request(addr: &str, method: &str, message: &str) -> HttpRequest {
    let mut r = HttpRequest::of_kind(RequestKind::Grpc);
    r.url = addr.to_owned();
    r.specs.grpc.method = method.to_owned();
    r.specs.grpc.message = message.to_owned();
    r
}

#[tokio::test]
async fn reflection_lists_methods_and_unary_calls_carry_metadata_and_auth() {
    let dir = proto_dir();
    let addr = server(pool(&dir)).await;
    let mut r = request(&addr, "test.Greeter/SayHello", r#"{"name": "Ada"}"#);
    r.headers.push(KeyValue::new("X-Team", "core"));
    r.auth = Auth::Bearer {
        token: "t0k".into(),
    };

    let schema = load_schema(&r).await.expect("reflection");
    let found = methods(&schema);
    let names: Vec<_> = found
        .iter()
        .map(|m| (m.full_name.as_str(), m.shape()))
        .collect();
    assert_eq!(
        names,
        [
            ("test.Greeter/Count", "server streaming"),
            ("test.Greeter/Fail", "unary"),
            ("test.Greeter/SayHello", "unary"),
            ("test.Greeter/Upload", "client streaming"),
        ]
    );
    let template: serde_json::Value =
        serde_json::from_str(&found[2].input_template).expect("template json");
    assert_eq!(template, serde_json::json!({"name": "", "times": 0}));

    let resp = invoke(&r, &schema, CancellationToken::new())
        .await
        .expect("call");
    let grpc = resp.grpc.clone().expect("status");
    assert_eq!((grpc.code, grpc.name.as_str()), (0, "OK"));
    let body: serde_json::Value = serde_json::from_slice(&resp.body).expect("json");
    assert_eq!(body["message"], "Hello, Ada!");
    assert_eq!(body["auth"], "Bearer t0k");
    assert_eq!(body["team"], "core");
    assert_eq!(resp.header("x-served-by"), Some("test"));
    assert!(
        resp.trailers
            .iter()
            .any(|(k, v)| k == "grpc-status" && v == "0")
    );
}

#[tokio::test]
async fn server_streaming_errors_and_unsupported_shapes() {
    let dir = proto_dir();
    let addr = server(pool(&dir)).await;
    let schema = load_schema(&request(&addr, "", ""))
        .await
        .expect("reflection");

    let resp = invoke(
        &request(&addr, "test.Greeter/Count", r#"{"to": 3}"#),
        &schema,
        CancellationToken::new(),
    )
    .await
    .expect("stream");
    let body: serde_json::Value = serde_json::from_slice(&resp.body).expect("json");
    assert_eq!(body, serde_json::json!([{"n": 1}, {"n": 2}, {"n": 3}]));

    let resp = invoke(
        &request(&addr, "test.Greeter/Fail", "{}"),
        &schema,
        CancellationToken::new(),
    )
    .await
    .expect("a status is a response");
    let grpc = resp.grpc.expect("status");
    assert_eq!(
        (grpc.code, grpc.name.as_str(), grpc.message.as_str()),
        (5, "NOT_FOUND", "no such user")
    );

    let err = invoke(
        &request(&addr, "test.Greeter/Upload", "{}"),
        &schema,
        CancellationToken::new(),
    )
    .await
    .expect_err("client streaming");
    assert!(matches!(err, ExecError::InvalidRequest(_)));
    let err = invoke(
        &request(&addr, "test.Greeter/SayHello", r#"{"nope": 1}"#),
        &schema,
        CancellationToken::new(),
    )
    .await
    .expect_err("bad field");
    assert!(err.to_string().contains("test.HelloRequest"), "{err}");
    let err = invoke(
        &request(&addr, "test.Greeter/Missing", "{}"),
        &schema,
        CancellationToken::new(),
    )
    .await
    .expect_err("missing");
    assert!(err.to_string().contains("not found"));
}

#[tokio::test]
async fn proto_files_are_compiled_and_connection_errors_are_reported() {
    let dir = proto_dir();
    let addr = server(pool(&dir)).await;
    let mut r = request(&addr, "test.Greeter/SayHello", r#"{"name": "Bo"}"#);
    r.specs.grpc.source = GrpcSchemaSource::ProtoFile;
    r.specs.grpc.proto_file = dir.path().join("greeter.proto").display().to_string();
    let schema = load_schema(&r).await.expect("compile");
    let resp = invoke(&r, &schema, CancellationToken::new())
        .await
        .expect("call");
    let body: serde_json::Value = serde_json::from_slice(&resp.body).expect("json");
    assert_eq!(body["message"], "Hello, Bo!");

    r.specs.grpc.proto_file = dir.path().join("missing.proto").display().to_string();
    assert!(matches!(load_schema(&r).await, Err(ExecError::File(_))));

    let refused = request("127.0.0.1:9", "test.Greeter/SayHello", "{}");
    let err = invoke(&refused, &schema, CancellationToken::new())
        .await
        .expect_err("refused");
    assert!(matches!(err, ExecError::Connect(_)), "{err:?}");
    let cancel = CancellationToken::new();
    cancel.cancel();
    let err = invoke(&r, &schema, cancel).await.expect_err("cancelled");
    assert_eq!(err, ExecError::Cancelled);
}

//! Every HTTP-based kind end to end through AppServices against a local
//! axum server: variables, inherited auth, lowering, schema / WSDL /
//! $metadata loading and caching, tRPC unwrapping; WebSocket history.

#[path = "common/protocol_server.rs"]
mod protocol_server;

use ferrumapi_application::domain::protocol::SoapVersion;
use ferrumapi_application::domain::protocol::trpc::{TrpcOutcome, TrpcType, unwrap_response};
use ferrumapi_application::domain::{
    Auth, Collection, Environment, HttpRequest, Item, RequestKind, RequestOrigin, SavedRequest,
    Variable,
};
use ferrumapi_application::{AppServices, CancellationToken, Prepared, prepare};
use protocol_server::{Hits, start};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::Ordering;

struct Fixture {
    svc: AppServices,
    dir: std::path::PathBuf,
    collection: Collection,
    env: Environment,
}

impl Fixture {
    /// A collection with bearer auth `{{token}}`; call [`Self::base`] next.
    /// Created outside any runtime (AppServices blocks while starting).
    fn new() -> Self {
        let dir = std::env::temp_dir().join(format!("ferrumapi-proto-{}", uuid::Uuid::new_v4()));
        let svc = AppServices::start(&dir).expect("start");
        let mut collection = Collection::new("Mixed");
        collection.auth = Auth::Bearer {
            token: "{{token}}".into(),
        };
        let mut env = Environment::new("local");
        env.variables = vec![Variable::new("token", "s3cret")];
        Self {
            svc,
            dir,
            collection,
            env,
        }
    }

    fn base(&mut self, base: &str) {
        self.env.variables.push(Variable::new("base", base));
    }

    /// Saves `request` (auth inherited) and prepares it as a tab would.
    fn prepared(&mut self, request: HttpRequest) -> (Prepared, Option<RequestOrigin>) {
        let saved = SavedRequest::new("r", request.clone());
        let origin = Some(RequestOrigin {
            collection: self.collection.id,
            item: saved.id,
        });
        self.collection.items.push(Item::Request(saved));
        let p = prepare(
            &request,
            origin,
            std::slice::from_ref(&self.collection),
            &Environment::globals(),
            Some(&self.env),
        );
        (p, origin)
    }

    async fn send(&mut self, request: HttpRequest) -> Value {
        let (p, origin) = self.prepared(request.clone());
        let resp = self
            .svc
            .send(request, p, origin, CancellationToken::new())
            .await
            .expect("send");
        serde_json::from_slice(&resp.body).unwrap_or_else(|_| Value::String(resp.body_text()))
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

fn of(kind: RequestKind, url: &str) -> HttpRequest {
    let mut r = HttpRequest::of_kind(kind);
    r.url = url.into();
    r
}

fn rt() -> tokio::runtime::Runtime {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .expect("rt")
}

#[test]
fn graphql_post_get_introspection_and_schema_cache() {
    let mut f = Fixture::new();
    rt().block_on(async {
        let hits = Arc::new(Hits::default());
        f.base(&start(hits.clone()).await);

        let mut r = of(RequestKind::Graphql, "{{base}}/graphql");
        r.specs.graphql.query = "query Me { me { id } }".into();
        r.specs.graphql.variables = r#"{"token": "{{token}}"}"#.into();
        r.specs.graphql.operation_name = "Me".into();
        let body = f.send(r.clone()).await;
        assert_eq!(body["data"]["query"], "query Me { me { id } }");
        assert_eq!(body["data"]["variables"], json!({"token": "s3cret"}));
        assert_eq!(body["data"]["operationName"], "Me");
        assert_eq!(body["data"]["auth"], "Bearer s3cret");
        assert_eq!(body["data"]["contentType"], "application/json");

        r.specs.graphql.use_get = true;
        let body = f.send(r.clone()).await;
        let raw = body["data"]["rawQuery"].as_str().expect("query string");
        assert!(
            raw.starts_with("query=query%20Me") && raw.contains("operationName=Me"),
            "{raw}"
        );

        let (p, _) = f.prepared(r.clone());
        let schema = f
            .svc
            .graphql_schema(p.clone(), false)
            .await
            .expect("schema");
        assert_eq!(schema.query_type.as_deref(), Some("Query"));
        assert!(schema.find("User").is_some());
        let again = f
            .svc
            .graphql_schema(p.clone(), false)
            .await
            .expect("cached");
        assert_eq!(again, schema);
        assert_eq!(
            hits.introspection.load(Ordering::SeqCst),
            1,
            "second load is cached"
        );
        f.svc.graphql_schema(p, true).await.expect("refresh");
        assert_eq!(hits.introspection.load(Ordering::SeqCst), 2);
    });
}

#[test]
fn soap_envelope_action_and_wsdl_operations() {
    let mut f = Fixture::new();
    rt().block_on(async {
        f.base(&start(Arc::default()).await);
        let mut r = of(RequestKind::Soap, "{{base}}/soap");
        r.specs.soap.wsdl_url = "{{base}}/soap?wsdl".into();
        let (p, _) = f.prepared(r.clone());
        let ops = f.svc.wsdl_operations(p, false).await.expect("wsdl");
        let add = ops.iter().find(|o| o.name == "Add").expect("Add");
        r.specs.soap.action.clone_from(&add.action);
        r.specs.soap.envelope = ferrumapi_application::domain::protocol::soap::envelope(
            SoapVersion::V11,
            &add.sample_body
                .replace("<tns:intA>?", "<tns:intA>2")
                .replace("<tns:intB>?", "<tns:intB>3"),
        );
        let body = f.send(r).await;
        let text = body.as_str().expect("xml text");
        assert!(text.contains("<AddResult>5</AddResult>"), "{text}");
        assert!(
            text.contains("<Action>\"http://tempuri.org/Add\"</Action>"),
            "{text}"
        );
        assert!(
            text.contains("<Type>text/xml; charset=utf-8</Type>"),
            "{text}"
        );
    });
}

#[test]
fn odata_query_builder_and_metadata() {
    let mut f = Fixture::new();
    rt().block_on(async {
        f.base(&start(Arc::default()).await);
        let mut r = of(RequestKind::Odata, "{{base}}/odata");
        r.specs.odata.entity_set = "People".into();
        r.specs.odata.filter = "Age gt 30".into();
        r.specs.odata.select = "UserName,Age".into();
        r.specs.odata.top = "{{top}}".into();
        f.env.variables.push(Variable::new("top", "2"));
        let body = f.send(r.clone()).await;
        assert_eq!(
            body["rawQuery"],
            "$filter=Age%20gt%2030&$select=UserName,Age&$top=2"
        );
        assert_eq!(body["accept"], "application/json");
        let (p, _) = f.prepared(r);
        let meta = f.svc.odata_metadata(p, false).await.expect("metadata");
        let names: Vec<_> = meta.entity_sets.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, ["People", "Airports"]);
    });
}

#[test]
fn trpc_queries_mutations_batches_and_errors_unwrap() {
    let mut f = Fixture::new();
    rt().block_on(async {
        f.base(&start(Arc::default()).await);
        let mut r = of(RequestKind::Trpc, "{{base}}/trpc");
        r.specs.trpc.procedure = "user.byId".into();
        r.specs.trpc.input = r#"{"id": "{{token}}"}"#.into();
        let send = |f: &mut Fixture, r: HttpRequest| {
            let (p, origin) = f.prepared(r.clone());
            let svc = f.svc.clone();
            async move {
                let resp = svc
                    .send(r, p, origin, CancellationToken::new())
                    .await
                    .expect("send");
                unwrap_response(&resp.body).expect("trpc shape")
            }
        };
        let expected = json!({"procedure": "user.byId", "input": {"id": "s3cret"}});
        assert_eq!(
            send(&mut f, r.clone()).await,
            TrpcOutcome::Data(expected.clone())
        );
        r.specs.trpc.batch = true;
        assert_eq!(
            send(&mut f, r.clone()).await,
            TrpcOutcome::Data(expected.clone())
        );
        r.specs.trpc.procedure_type = TrpcType::Mutation;
        assert_eq!(send(&mut f, r.clone()).await, TrpcOutcome::Data(expected));
        r.specs.trpc.batch = false;
        r.specs.trpc.procedure = "fail".into();
        assert_eq!(
            send(&mut f, r).await,
            TrpcOutcome::Error(json!({"message": "nope", "code": -32600}))
        );
    });
}

#[test]
fn websocket_connections_are_recorded_and_not_sent_as_http() {
    let mut f = Fixture::new();
    rt().block_on(async {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind");
        let addr = listener.local_addr().expect("addr");
        tokio::spawn(async move {
            if let Ok((tcp, _)) = listener.accept().await
                && let Ok(mut ws) = tokio_tungstenite::accept_async(tcp).await
            {
                use futures::{SinkExt as _, StreamExt as _};
                while let Some(Ok(m)) = ws.next().await {
                    if m.is_text() && ws.send(m).await.is_err() {
                        break;
                    }
                }
            }
        });
        f.base(&format!("ws://{addr}"));
        let r = of(RequestKind::WebSocket, "{{base}}/socket");
        let (p, origin) = f.prepared(r.clone());
        let err = f
            .svc
            .execute(p.clone(), CancellationToken::new())
            .await
            .expect_err("not http");
        assert!(err.to_string().contains("Connect"));
        let (connected, client, mut rx) = f.svc.ws_connect(r, p, origin).await.expect("connect");
        assert_eq!(connected.status, 101);
        assert!(
            client.send(ferrumapi_application::http::ws::WsOutgoing::Text(
                "hi".into()
            ))
        );
        let echoed = rx.recv().await.expect("echo");
        assert_eq!(
            echoed,
            ferrumapi_application::http::ws::WsIncoming::Text("hi".into())
        );
        let history = f.svc.store.list_history(10).await.expect("history");
        assert_eq!(history[0].status, Some(101));
        assert_eq!(history[0].request.kind, RequestKind::WebSocket);
    });
}

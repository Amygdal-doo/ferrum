//! A local axum server speaking GraphQL, SOAP (+ WSDL), OData and tRPC,
//! with hit counters, for the protocol integration tests.

use axum::Router;
use axum::body::Bytes;
use axum::extract::{Path, RawQuery, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::{get, post};
use serde_json::{Value, json};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

pub const WSDL: &str = include_str!("../../../interop/tests/fixtures/calculator.wsdl");
pub const METADATA: &str = include_str!("../../../interop/tests/fixtures/odata_metadata.xml");

#[derive(Default)]
pub struct Hits {
    pub introspection: AtomicUsize,
}

fn header(h: &HeaderMap, name: &str) -> String {
    h.get(name)
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_owned()
}

fn introspection() -> Value {
    json!({"data": {"__schema": {
    "queryType": {"name": "Query"}, "mutationType": null, "subscriptionType": null,
    "types": [
        {"kind": "OBJECT", "name": "Query", "fields": [
            {"name": "me", "args": [], "type": {"kind": "OBJECT", "name": "User"}}]},
        {"kind": "OBJECT", "name": "User", "fields": [
            {"name": "id", "args": [], "type": {"kind": "SCALAR", "name": "ID"}},
            {"name": "name", "args": [], "type": {"kind": "SCALAR", "name": "String"}}]}
    ]}}})
}

async fn graphql_post(
    State(hits): State<Arc<Hits>>,
    h: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    let doc: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    let query = doc["query"].as_str().unwrap_or_default();
    if query.contains("__schema") {
        hits.introspection.fetch_add(1, Ordering::SeqCst);
        return axum::Json(introspection());
    }
    axum::Json(json!({"data": {
        "query": query,
        "variables": doc.get("variables").cloned().unwrap_or(Value::Null),
        "operationName": doc.get("operationName").cloned().unwrap_or(Value::Null),
        "auth": header(&h, "authorization"),
        "contentType": header(&h, "content-type"),
    }}))
}

async fn graphql_get(RawQuery(q): RawQuery) -> impl IntoResponse {
    axum::Json(json!({"data": {"rawQuery": q}}))
}

async fn soap(h: HeaderMap, body: String) -> impl IntoResponse {
    let action = header(&h, "soapaction");
    let ctype = header(&h, "content-type");
    let ok = body.contains("<soap:Envelope") && body.contains("<tns:intA>2</tns:intA>");
    let xml = format!(
        "<?xml version=\"1.0\"?><soap:Envelope xmlns:soap=\"http://schemas.xmlsoap.org/soap/envelope/\"><soap:Body><AddResponse><AddResult>{}</AddResult><Action>{action}</Action><Type>{ctype}</Type></AddResponse></soap:Body></soap:Envelope>",
        if ok { 5 } else { 0 }
    );
    ([("content-type", "text/xml; charset=utf-8")], xml)
}

async fn wsdl(RawQuery(q): RawQuery) -> impl IntoResponse {
    if q.as_deref() == Some("wsdl") {
        ([("content-type", "text/xml")], WSDL).into_response()
    } else {
        StatusCode::NOT_FOUND.into_response()
    }
}

async fn odata_set(
    Path(set): Path<String>,
    RawQuery(q): RawQuery,
    h: HeaderMap,
) -> impl IntoResponse {
    if set == "$metadata" {
        return ([("content-type", "application/xml")], METADATA).into_response();
    }
    axum::Json(json!({
        "@odata.context": format!("$metadata#{set}"),
        "rawQuery": q,
        "accept": header(&h, "accept"),
        "value": [{"UserName": "russell"}],
    }))
    .into_response()
}

fn trpc_result(procedure: &str, input: Value) -> Value {
    json!({"result": {"data": {"procedure": procedure, "input": input}}})
}

async fn trpc_get(Path(procedure): Path<String>, RawQuery(q): RawQuery) -> impl IntoResponse {
    let q = q.unwrap_or_default();
    let batch = q.contains("batch=1");
    let input = q
        .split('&')
        .find_map(|p| p.strip_prefix("input="))
        .map(|enc| {
            let text = percent_decode(enc);
            serde_json::from_str::<Value>(&text).unwrap_or(Value::Null)
        })
        .unwrap_or(Value::Null);
    if batch {
        axum::Json(json!([trpc_result(&procedure, input["0"].clone())]))
    } else {
        axum::Json(trpc_result(&procedure, input))
    }
}

async fn trpc_post(
    Path(procedure): Path<String>,
    RawQuery(q): RawQuery,
    body: Bytes,
) -> impl IntoResponse {
    let input: Value = serde_json::from_slice(&body).unwrap_or(Value::Null);
    if procedure == "fail" {
        return (
            StatusCode::BAD_REQUEST,
            axum::Json(json!({"error": {"message": "nope", "code": -32600}})),
        )
            .into_response();
    }
    if q.unwrap_or_default().contains("batch=1") {
        axum::Json(json!([trpc_result(&procedure, input["0"].clone())])).into_response()
    } else {
        axum::Json(trpc_result(&procedure, input)).into_response()
    }
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%'
            && i + 2 < bytes.len()
            && let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16)
        {
            out.push(b);
            i += 3;
            continue;
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Starts the server on 127.0.0.1 and returns its base URL.
pub async fn start(hits: Arc<Hits>) -> String {
    let app = Router::new()
        .route("/graphql", post(graphql_post).get(graphql_get))
        .route("/soap", post(soap).get(wsdl))
        .route("/odata/{set}", get(odata_set))
        .route("/trpc/{procedure}", get(trpc_get).post(trpc_post))
        .with_state(hits);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind");
    let addr = listener.local_addr().expect("addr");
    tokio::spawn(async move {
        axum::serve(listener, app).await.expect("serve");
    });
    format!("http://{addr}")
}

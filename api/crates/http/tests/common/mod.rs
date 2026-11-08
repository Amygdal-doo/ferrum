//! Shared helpers for integration tests (local wiremock server only).
#![allow(dead_code)]

use ferrumapi_domain::{ExecError, HttpMethod, HttpRequest, HttpResponse, VariableSet};
use ferrumapi_http::{CancellationToken, HttpClient};
use wiremock::matchers::any;
use wiremock::{Mock, MockServer, Request, ResponseTemplate};

/// A server answering 200 "ok" to everything.
pub async fn ok_server() -> MockServer {
    let server = MockServer::start().await;
    Mock::given(any())
        .respond_with(ResponseTemplate::new(200).set_body_string("ok"))
        .mount(&server)
        .await;
    server
}

pub fn req(method: HttpMethod, url: impl Into<String>) -> HttpRequest {
    HttpRequest::new(method, url)
}

pub async fn send(request: &HttpRequest) -> Result<HttpResponse, ExecError> {
    send_vars(request, &VariableSet::new()).await
}

pub async fn send_vars(
    request: &HttpRequest,
    vars: &VariableSet,
) -> Result<HttpResponse, ExecError> {
    HttpClient::new()
        .execute(request, vars, CancellationToken::new())
        .await
}

/// The single request the server received.
pub async fn only_request(server: &MockServer) -> Request {
    let mut all = server.received_requests().await.expect("recording enabled");
    assert_eq!(all.len(), 1, "expected exactly one request");
    all.remove(0)
}

pub fn header<'a>(r: &'a Request, name: &str) -> Option<&'a str> {
    r.headers
        .get(name)
        .map(|v| v.to_str().expect("ascii header"))
}

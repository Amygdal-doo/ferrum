//! Command lines for the kinds that are not one HTTP request: `grpcurl`
//! for gRPC and `websocat` for WebSocket.

use super::Prepared;
use super::curl::quote;
use ferrumapi_domain::HttpRequest;
use ferrumapi_domain::protocol::grpc::endpoint;

fn headers(p: &Prepared<'_>) -> Vec<(String, String)> {
    let mut out = p.headers.clone();
    if let Some(basic) = p.basic_header() {
        out.push(("Authorization".into(), basic));
    }
    out
}

pub(super) fn grpcurl(p: &Prepared<'_>, r: &HttpRequest) -> String {
    let spec = &r.specs.grpc;
    let (url, tls) = endpoint(&r.url, spec.tls);
    let authority = url
        .split_once("://")
        .map_or(url.as_str(), |(_, rest)| rest)
        .to_owned();
    let mut parts = vec!["grpcurl".to_owned()];
    if !tls {
        parts.push("-plaintext".into());
    } else if !r.settings.verify_tls {
        parts.push("-insecure".into());
    }
    for (k, v) in headers(p) {
        parts.push(format!("-H {}", quote(&format!("{k}: {v}"))));
    }
    if spec.source == ferrumapi_domain::protocol::grpc::GrpcSchemaSource::ProtoFile {
        for dir in spec
            .import_paths
            .lines()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        {
            parts.push(format!("-import-path {}", quote(dir)));
        }
        if !spec.proto_file.trim().is_empty() {
            parts.push(format!("-proto {}", quote(spec.proto_file.trim())));
        }
    }
    let message = spec.message.trim();
    if !message.is_empty() {
        parts.push(format!("-d {}", quote(message)));
    }
    parts.push(quote(&authority));
    parts.push(quote(spec.method.trim().trim_start_matches('/')));
    parts.join(" \\\n  ")
}

pub(super) fn websocat(p: &Prepared<'_>, r: &HttpRequest) -> String {
    let spec = &r.specs.websocket;
    let mut parts = vec!["websocat".to_owned()];
    if !r.settings.verify_tls {
        parts.push("--insecure".into());
    }
    for (k, v) in headers(p) {
        parts.push(format!("-H {}", quote(&format!("{k}: {v}"))));
    }
    let protocols = spec.subprotocol_list();
    if !protocols.is_empty() {
        parts.push(format!("--protocol {}", quote(&protocols.join(", "))));
    }
    parts.push(quote(p.url.trim()));
    parts.join(" \\\n  ")
}

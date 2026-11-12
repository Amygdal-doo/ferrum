//! Calling a method: JSON → dynamic message → unary / server-streaming
//! call → JSON, with headers, trailers and the gRPC status.

use crate::channel::connect;
use crate::codec::DynamicCodec;
use crate::schema::with_metadata;
use ferrumapi_domain::protocol::grpc::{GrpcStatus, split_method, status_name};
use ferrumapi_domain::{ExecError, HttpRequest, HttpResponse};
use prost_reflect::{DescriptorPool, DynamicMessage, SerializeOptions};
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;
use tonic::codegen::http::uri::PathAndQuery;
use tonic::metadata::{KeyAndValueRef, MetadataMap};

fn pairs(map: &MetadataMap) -> Vec<(String, String)> {
    map.iter()
        .map(|kv| match kv {
            KeyAndValueRef::Ascii(k, v) => (
                k.to_string(),
                v.to_str()
                    .map_or_else(|_| "(non-ASCII)".to_owned(), str::to_owned),
            ),
            KeyAndValueRef::Binary(k, v) => (k.to_string(), format!("{:?}", v.as_encoded_bytes())),
        })
        .collect()
}

fn to_json(messages: &[DynamicMessage], streaming: bool) -> String {
    let options = SerializeOptions::new().skip_default_fields(false);
    let values: Vec<serde_json::Value> = messages
        .iter()
        .map(|m| {
            m.serialize_with_options(serde_json::value::Serializer, &options)
                .unwrap_or(serde_json::Value::Null)
        })
        .collect();
    let value = if streaming {
        serde_json::Value::Array(values)
    } else {
        values.into_iter().next().unwrap_or(serde_json::Value::Null)
    };
    serde_json::to_string_pretty(&value).unwrap_or_default()
}

/// Calls `request.specs.grpc.method` with its JSON message. `request` must
/// be substituted with auth effective. A non-OK gRPC status is a response
/// (with `grpc.code != 0`), not an error.
pub async fn invoke(
    request: &HttpRequest,
    pool: &DescriptorPool,
    cancel: CancellationToken,
) -> Result<HttpResponse, ExecError> {
    let spec = &request.specs.grpc;
    let (service, method) = split_method(&spec.method).ok_or_else(|| {
        ExecError::InvalidRequest("choose a method (package.Service/Method)".into())
    })?;
    let descriptor = pool
        .get_service_by_name(service)
        .and_then(|s| s.methods().find(|m| m.name() == method))
        .ok_or_else(|| ExecError::InvalidRequest(format!("method {service}/{method} not found")))?;
    if descriptor.is_client_streaming() {
        return Err(ExecError::InvalidRequest(
            "client-streaming and bidi-streaming methods are not supported yet".into(),
        ));
    }
    let text = match spec.message.trim() {
        "" => "{}",
        t => t,
    };
    let mut de = serde_json::Deserializer::from_str(text);
    let message = DynamicMessage::deserialize(descriptor.input(), &mut de)
        .and_then(|m| de.end().map(|()| m))
        .map_err(|e| {
            ExecError::InvalidRequest(format!(
                "the message does not match {}: {e}",
                descriptor.input().full_name()
            ))
        })?;
    let metadata = ferrumapi_http::metadata_pairs(request)?;
    let streaming = descriptor.is_server_streaming();
    let path = PathAndQuery::try_from(format!("/{service}/{method}"))
        .map_err(|e| ExecError::InvalidRequest(e.to_string()))?;
    let started = Instant::now();
    let work = async {
        let channel = connect(&request.url, spec.tls, request.settings.verify_tls).await?;
        let mut grpc = tonic::client::Grpc::new(channel);
        grpc.ready()
            .await
            .map_err(|e| ExecError::Connect(e.to_string()))?;
        let codec = DynamicCodec::new(descriptor.output());
        let mut messages = Vec::new();
        let (headers, trailers, status) = match grpc
            .server_streaming(with_metadata(message, &metadata), path, codec)
            .await
        {
            Ok(resp) => {
                let headers = pairs(resp.metadata());
                let mut stream = resp.into_inner();
                let failed = loop {
                    match stream.message().await {
                        Ok(Some(m)) => messages.push(m),
                        Ok(None) => break None,
                        Err(s) => break Some(s),
                    }
                };
                match failed {
                    Some(s) => (headers, pairs(s.metadata()), s),
                    None => {
                        let trailers = stream.trailers().await.ok().flatten();
                        let trailers = trailers.as_ref().map(pairs).unwrap_or_default();
                        (headers, trailers, tonic::Status::ok(""))
                    }
                }
            }
            Err(s) => (Vec::new(), pairs(s.metadata()), s),
        };
        let code = status.code() as i32;
        let body = to_json(&messages, streaming).into_bytes();
        Ok(HttpResponse {
            status: 200,
            status_text: status_name(code).to_owned(),
            http_version: "HTTP/2.0 (gRPC)".to_owned(),
            final_url: request.url.trim().to_owned(),
            headers: [("content-type".to_owned(), "application/json".to_owned())]
                .into_iter()
                .chain(headers)
                .collect(),
            size_bytes: body.len() as u64,
            body,
            time_ms: u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX),
            grpc: Some(GrpcStatus {
                code,
                name: status_name(code).to_owned(),
                message: status.message().to_owned(),
            }),
            trailers,
            ..HttpResponse::default()
        })
    };
    let timeout_ms = request.settings.timeout_ms;
    let timed = async {
        if timeout_ms == 0 {
            work.await
        } else {
            tokio::time::timeout(Duration::from_millis(timeout_ms), work)
                .await
                .unwrap_or(Err(ExecError::Timeout(timeout_ms)))
        }
    };
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(ExecError::Cancelled),
        r = timed => r,
    }
}

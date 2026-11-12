//! Where method descriptors come from: server reflection or a .proto file.

use crate::channel::connect;
use crate::status_error;
use ferrumapi_domain::protocol::grpc::{GrpcMethodInfo, GrpcSchemaSource};
use ferrumapi_domain::{ExecError, HttpRequest};
use futures::StreamExt as _;
use prost::Message as _;
use prost_reflect::{DescriptorPool, DynamicMessage, SerializeOptions};
use prost_types::FileDescriptorProto;
use std::collections::{BTreeMap, VecDeque};
use std::path::Path;
use tonic::metadata::{AsciiMetadataValue, MetadataKey};
use tonic::transport::Channel;

/// Descriptors for `request` (already substituted, auth effective).
pub async fn load_schema(request: &HttpRequest) -> Result<DescriptorPool, ExecError> {
    let spec = &request.specs.grpc;
    match spec.source {
        GrpcSchemaSource::ProtoFile => {
            let includes: Vec<&str> = spec
                .import_paths
                .lines()
                .map(str::trim)
                .filter(|l| !l.is_empty())
                .collect();
            compile_proto(spec.proto_file.trim(), &includes)
        }
        GrpcSchemaSource::Reflection => {
            let channel = connect(&request.url, spec.tls, request.settings.verify_tls).await?;
            let metadata = ferrumapi_http::metadata_pairs(request)?;
            reflect(channel, &metadata).await
        }
    }
}

/// Compiles `file` (and its imports, searched in `includes` and the file's
/// folder) with protox.
pub fn compile_proto(file: &str, includes: &[&str]) -> Result<DescriptorPool, ExecError> {
    if file.is_empty() {
        return Err(ExecError::InvalidRequest("choose a .proto file".into()));
    }
    let path = Path::new(file);
    let mut dirs: Vec<&Path> = includes.iter().map(Path::new).collect();
    if let Some(parent) = path.parent() {
        dirs.push(parent);
    }
    let name = path
        .file_name()
        .map_or(file, |n| n.to_str().unwrap_or(file));
    let set = protox::compile([name], dirs)
        .map_err(|e| ExecError::File(format!("could not compile the .proto file: {e}")))?;
    DescriptorPool::from_file_descriptor_set(set)
        .map_err(|e| ExecError::File(format!("invalid descriptors: {e}")))
}

/// Every method of every service, sorted, with an input template.
#[must_use]
pub fn methods(pool: &DescriptorPool) -> Vec<GrpcMethodInfo> {
    let options = SerializeOptions::new().skip_default_fields(false);
    let mut out: Vec<GrpcMethodInfo> = pool
        .services()
        .filter(|s| !s.full_name().starts_with("grpc.reflection."))
        .flat_map(|s| s.methods().collect::<Vec<_>>())
        .map(|m| {
            let template = DynamicMessage::new(m.input());
            let mut buf = Vec::new();
            let mut ser = serde_json::Serializer::pretty(&mut buf);
            let input_template = match template.serialize_with_options(&mut ser, &options) {
                Ok(()) => String::from_utf8(buf).unwrap_or_default(),
                Err(_) => "{}".to_owned(),
            };
            GrpcMethodInfo {
                full_name: format!("{}/{}", m.parent_service().full_name(), m.name()),
                client_streaming: m.is_client_streaming(),
                server_streaming: m.is_server_streaming(),
                input_type: m.input().full_name().to_owned(),
                output_type: m.output().full_name().to_owned(),
                input_template,
            }
        })
        .collect();
    out.sort_by(|a, b| a.full_name.cmp(&b.full_name));
    out
}

pub(crate) fn with_metadata<T>(message: T, metadata: &[(String, String)]) -> tonic::Request<T> {
    let mut req = tonic::Request::new(message);
    for (k, v) in metadata {
        if let (Ok(key), Ok(value)) = (
            MetadataKey::from_bytes(k.to_ascii_lowercase().as_bytes()),
            AsciiMetadataValue::try_from(v.as_str()),
        ) {
            req.metadata_mut().append(key, value);
        }
    }
    req
}

/// Reflection is the same protocol in `v1` and `v1alpha` with different
/// generated types; this runs one conversation with either.
macro_rules! reflect_with {
    ($pb:path, $channel:expr, $metadata:expr) => {
        async {
            use pb::server_reflection_client::ServerReflectionClient;
            use pb::server_reflection_request::MessageRequest;
            use pb::server_reflection_response::MessageResponse;
            use $pb as pb;

            let mut client = ServerReflectionClient::new($channel);
            let (tx, rx) = tokio::sync::mpsc::unbounded_channel::<pb::ServerReflectionRequest>();
            let stream =
                futures::stream::unfold(
                    rx,
                    |mut rx| async move { rx.recv().await.map(|m| (m, rx)) },
                );
            let send = |m: MessageRequest| {
                tx.send(pb::ServerReflectionRequest {
                    host: String::new(),
                    message_request: Some(m),
                })
                .is_ok()
            };
            send(MessageRequest::ListServices(String::new()));
            let mut responses = client
                .server_reflection_info(with_metadata(stream, $metadata))
                .await?
                .into_inner();
            let mut files: BTreeMap<String, FileDescriptorProto> = BTreeMap::new();
            let mut pending: VecDeque<String> = VecDeque::new();
            let mut in_flight = 0usize;
            // ListServices first, then one request per symbol / missing file.
            let mut services_seen = false;
            while let Some(resp) = responses.next().await {
                let resp = resp?;
                match resp.message_response {
                    Some(MessageResponse::ListServicesResponse(list)) => {
                        services_seen = true;
                        for s in list.service {
                            if !s.name.starts_with("grpc.reflection.") {
                                in_flight +=
                                    usize::from(send(MessageRequest::FileContainingSymbol(s.name)));
                            }
                        }
                    }
                    Some(MessageResponse::FileDescriptorResponse(fd)) => {
                        in_flight = in_flight.saturating_sub(1);
                        for bytes in fd.file_descriptor_proto {
                            let file =
                                FileDescriptorProto::decode(bytes.as_slice()).map_err(|e| {
                                    tonic::Status::internal(format!("bad descriptor: {e}"))
                                })?;
                            for dep in &file.dependency {
                                if !files.contains_key(dep) && !pending.contains(dep) {
                                    pending.push_back(dep.clone());
                                }
                            }
                            files.insert(file.name().to_owned(), file);
                        }
                    }
                    Some(MessageResponse::ErrorResponse(e)) => {
                        in_flight = in_flight.saturating_sub(1);
                        tracing::debug!(code = e.error_code, "reflection lookup failed");
                    }
                    _ => in_flight = in_flight.saturating_sub(1),
                }
                pending.retain(|p| !files.contains_key(p));
                if let Some(next) = pending.pop_front() {
                    in_flight += usize::from(send(MessageRequest::FileByFilename(next)));
                }
                if services_seen && in_flight == 0 && pending.is_empty() {
                    break;
                }
            }
            drop(tx);
            Ok::<_, tonic::Status>(files.into_values().collect::<Vec<_>>())
        }
        .await
    };
}

/// Asks the server for its services and their files (v1, then v1alpha).
pub async fn reflect(
    channel: Channel,
    metadata: &[(String, String)],
) -> Result<DescriptorPool, ExecError> {
    let files = match reflect_with!(tonic_reflection::pb::v1, channel.clone(), metadata) {
        Ok(files) => files,
        Err(s) if s.code() == tonic::Code::Unimplemented => {
            reflect_with!(tonic_reflection::pb::v1alpha, channel, metadata).map_err(|s| {
                if s.code() == tonic::Code::Unimplemented {
                    ExecError::Other(
                        "the server does not support reflection; load a .proto file instead".into(),
                    )
                } else {
                    status_error(&s)
                }
            })?
        }
        Err(s) => return Err(status_error(&s)),
    };
    let mut pool = DescriptorPool::new();
    pool.add_file_descriptor_protos(files)
        .map_err(|e| ExecError::Other(format!("could not read the server's descriptors: {e}")))?;
    Ok(pool)
}

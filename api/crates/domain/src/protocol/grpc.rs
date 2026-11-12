//! gRPC request settings. Messages are edited as JSON and converted with
//! the method's descriptor (from server reflection or a .proto file).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrpcSchemaSource {
    /// Ask the server (grpc.reflection.v1 / v1alpha).
    #[default]
    Reflection,
    /// Compile a local .proto file.
    ProtoFile,
}

impl GrpcSchemaSource {
    pub const ALL: [Self; 2] = [Self::Reflection, Self::ProtoFile];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reflection => "Server reflection",
            Self::ProtoFile => ".proto file",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcSpec {
    /// Fully qualified, e.g. `helloworld.Greeter/SayHello`.
    pub method: String,
    /// Request message as JSON (proto3 JSON mapping).
    pub message: String,
    pub source: GrpcSchemaSource,
    /// Absolute path of the .proto file (source = ProtoFile).
    pub proto_file: String,
    /// Extra include directories for imports, one per line.
    pub import_paths: String,
    /// Use TLS even when the URL has no `https://` / `grpcs://` scheme.
    pub tls: bool,
}

/// `(service, method)` of `pkg.Service/Method` (also accepts `pkg.Service.Method`).
#[must_use]
pub fn split_method(full: &str) -> Option<(&str, &str)> {
    let full = full.trim().trim_start_matches('/');
    let (service, method) = full.rsplit_once('/').or_else(|| full.rsplit_once('.'))?;
    (!service.is_empty() && !method.is_empty()).then_some((service, method))
}

/// `(endpoint URL for the transport, uses TLS)` from what the user typed:
/// `grpcs://` / `https://` mean TLS, `grpc://` / `http://` / none plaintext.
#[must_use]
pub fn endpoint(raw: &str, force_tls: bool) -> (String, bool) {
    let raw = raw.trim().trim_end_matches('/');
    let (rest, tls) = if let Some(r) = raw.strip_prefix("grpcs://") {
        (r, true)
    } else if let Some(r) = raw.strip_prefix("https://") {
        (r, true)
    } else if let Some(r) = raw.strip_prefix("grpc://") {
        (r, false)
    } else if let Some(r) = raw.strip_prefix("http://") {
        (r, false)
    } else {
        (raw, false)
    };
    let tls = tls || force_tls;
    let scheme = if tls { "https" } else { "http" };
    (format!("{scheme}://{rest}"), tls)
}

/// One method of a service, as found by reflection or in a .proto file.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcMethodInfo {
    /// `pkg.Service/Method`.
    pub full_name: String,
    pub client_streaming: bool,
    pub server_streaming: bool,
    pub input_type: String,
    pub output_type: String,
    /// The input message with every field at its default, as JSON.
    pub input_template: String,
}

impl GrpcMethodInfo {
    /// `unary`, `server streaming`, …
    #[must_use]
    pub const fn shape(&self) -> &'static str {
        match (self.client_streaming, self.server_streaming) {
            (false, false) => "unary",
            (false, true) => "server streaming",
            (true, false) => "client streaming",
            (true, true) => "bidi streaming",
        }
    }
}

/// The gRPC status of a call.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GrpcStatus {
    pub code: i32,
    /// e.g. `OK`, `NOT_FOUND`.
    pub name: String,
    pub message: String,
}

/// Canonical name of a gRPC status code.
#[must_use]
pub const fn status_name(code: i32) -> &'static str {
    match code {
        0 => "OK",
        1 => "CANCELLED",
        2 => "UNKNOWN",
        3 => "INVALID_ARGUMENT",
        4 => "DEADLINE_EXCEEDED",
        5 => "NOT_FOUND",
        6 => "ALREADY_EXISTS",
        7 => "PERMISSION_DENIED",
        8 => "RESOURCE_EXHAUSTED",
        9 => "FAILED_PRECONDITION",
        10 => "ABORTED",
        11 => "OUT_OF_RANGE",
        12 => "UNIMPLEMENTED",
        13 => "INTERNAL",
        14 => "UNAVAILABLE",
        15 => "DATA_LOSS",
        16 => "UNAUTHENTICATED",
        _ => "UNKNOWN",
    }
}

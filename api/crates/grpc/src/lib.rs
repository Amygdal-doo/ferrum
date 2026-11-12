//! gRPC for FerrumAPI: messages are JSON in the editor and dynamic protobuf
//! messages on the wire, described by server reflection
//! (`grpc.reflection.v1`, falling back to `v1alpha`) or a local `.proto`
//! file compiled in pure Rust (protox).
//!
//! Unary and server-streaming methods can be invoked; client and bidi
//! streaming are reported as unsupported. Nothing here logs addresses,
//! metadata or messages.

mod channel;
pub mod codec;
mod invoke;
mod schema;

pub use invoke::invoke;
pub use prost_reflect::DescriptorPool;
pub use schema::{compile_proto, load_schema, methods, reflect};

use ferrumapi_domain::ExecError;

/// A gRPC failure that is not a call status (those become responses).
fn status_error(status: &tonic::Status) -> ExecError {
    let code = status.code();
    if code == tonic::Code::Unavailable {
        return ExecError::Connect(status.message().to_owned());
    }
    ExecError::Other(format!(
        "gRPC {}: {}",
        ferrumapi_domain::protocol::grpc::status_name(code as i32),
        status.message()
    ))
}

//! Import/export and code generation:
//! - [`postman`]: Postman Collection v2.1 (v2.0 accepted) and environments,
//! - [`curl`]: `curl ...` command lines → requests,
//! - [`codegen`]: requests → cURL / Python requests / JavaScript fetch snippets,
//! - [`wsdl`]: WSDL 1.1 → SOAP operations with sample envelopes,
//! - [`odata`]: OData `$metadata` → entity sets.

pub mod codegen;
pub mod curl;
pub mod odata;
pub mod postman;
pub mod wsdl;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum InteropError {
    #[error("invalid JSON: {0}")]
    Json(String),
    #[error("unsupported format: {0}")]
    Unsupported(String),
    #[error("could not parse: {0}")]
    Parse(String),
}

//! GraphQL over HTTP: POST `{"query", "variables", "operationName"}` as JSON,
//! or GET with the same fields as query parameters.

use super::{append_query, default_header, encode_component, optional_json};
use crate::body::{RawLanguage, RequestBody};
use crate::error::ExecError;
use crate::request::{HttpMethod, HttpRequest};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphqlSpec {
    pub query: String,
    /// JSON object text; blank means no variables.
    pub variables: String,
    pub operation_name: String,
    /// Send as GET with query parameters instead of a POST JSON body.
    pub use_get: bool,
}

/// The JSON document a POST sends.
pub fn document(spec: &GraphqlSpec) -> Result<Value, ExecError> {
    let mut doc = Map::new();
    doc.insert("query".into(), Value::String(spec.query.clone()));
    if let Some(vars) = variables(spec)? {
        doc.insert("variables".into(), vars);
    }
    let op = spec.operation_name.trim();
    if !op.is_empty() {
        doc.insert("operationName".into(), Value::String(op.to_owned()));
    }
    Ok(Value::Object(doc))
}

fn variables(spec: &GraphqlSpec) -> Result<Option<Value>, ExecError> {
    match optional_json(&spec.variables, "GraphQL variables")? {
        None | Some(Value::Null) => Ok(None),
        Some(v @ Value::Object(_)) => Ok(Some(v)),
        Some(_) => Err(ExecError::InvalidRequest(
            "GraphQL variables must be a JSON object".into(),
        )),
    }
}

pub(super) fn lower(request: &HttpRequest) -> Result<HttpRequest, ExecError> {
    let spec = &request.specs.graphql;
    if spec.query.trim().is_empty() {
        return Err(ExecError::InvalidRequest(
            "the GraphQL query is empty".into(),
        ));
    }
    let mut out = request.clone();
    if spec.use_get {
        let mut pairs = vec![("query".to_owned(), encode_component(&spec.query))];
        if let Some(vars) = variables(spec)? {
            pairs.push(("variables".into(), encode_component(&vars.to_string())));
        }
        let op = spec.operation_name.trim();
        if !op.is_empty() {
            pairs.push(("operationName".into(), encode_component(op)));
        }
        out.method = HttpMethod::Get;
        out.url = append_query(&request.url, &pairs);
        out.body = RequestBody::None;
    } else {
        out.method = HttpMethod::Post;
        out.body = RequestBody::Raw {
            language: RawLanguage::Json,
            text: document(spec)?.to_string(),
        };
    }
    default_header(
        &mut out.headers,
        "Accept",
        "application/graphql-response+json, application/json",
    );
    Ok(out)
}

/// The standard introspection query (types, fields, arguments, enum values,
/// input fields; deep enough for list / non-null wrappers).
pub const INTROSPECTION_QUERY: &str = r"query IntrospectionQuery {
  __schema {
    queryType { name }
    mutationType { name }
    subscriptionType { name }
    types {
      kind name description
      fields(includeDeprecated: true) {
        name description
        args { name description type { ...TypeRef } defaultValue }
        type { ...TypeRef }
      }
      inputFields { name description type { ...TypeRef } defaultValue }
      enumValues(includeDeprecated: true) { name description }
    }
  }
}
fragment TypeRef on __Type {
  kind name
  ofType { kind name ofType { kind name ofType { kind name ofType { kind name
    ofType { kind name ofType { kind name ofType { kind name } } } } } } }
}";

/// The introspection request for `request`'s endpoint, headers and auth.
#[must_use]
pub fn introspection_request(request: &HttpRequest) -> HttpRequest {
    let mut out = request.clone();
    out.specs.graphql = GraphqlSpec {
        query: INTROSPECTION_QUERY.to_owned(),
        operation_name: "IntrospectionQuery".to_owned(),
        ..GraphqlSpec::default()
    };
    out
}

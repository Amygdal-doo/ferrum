//! Postman environment files.

use super::text;
use crate::InteropError;
use ferrumapi_domain::{Environment, Variable};
use serde_json::Value;

pub(super) fn import(v: &Value) -> Result<Environment, InteropError> {
    let values = v.get("values").and_then(Value::as_array).ok_or_else(|| {
        InteropError::Unsupported("not a Postman environment (missing `values`)".into())
    })?;
    if v.get("info").is_some() {
        return Err(InteropError::Unsupported(
            "this is a Postman collection, not an environment".into(),
        ));
    }
    let name = text(v.get("name"));
    let mut env = Environment::new(if name.is_empty() {
        "Imported environment".to_owned()
    } else {
        name
    });
    env.variables = values
        .iter()
        .filter(|row| row.is_object())
        .map(|row| Variable {
            key: text(row.get("key")),
            value: text(row.get("value")),
            enabled: row.get("enabled").and_then(Value::as_bool).unwrap_or(true),
            secret: row.get("type").and_then(Value::as_str) == Some("secret"),
        })
        .collect();
    Ok(env)
}

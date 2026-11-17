//! JSON Schema: validate a document against a schema (the `jsonschema`
//! crate, drafts 4 to 2020-12, picked from `$schema`; no remote `$ref`s
//! are fetched) and infer a schema from a sample document.

use serde_json::{Map, Value, json};

/// One validation failure.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SchemaError {
    /// JSON Pointer into the document (`/items/0/name`; empty for the root).
    pub path: String,
    pub message: String,
}

fn parse(text: &str, what: &str) -> Result<Value, String> {
    serde_json::from_str(text).map_err(|e| format!("The {what} is not valid JSON: {e}"))
}

/// Validates `document` against `schema` (both JSON text). `Ok` holds the
/// failures (empty when valid); `Err` is for unparsable input or an
/// invalid schema. Formats (`email`, `date-time`, …) are checked.
pub fn validate(schema: &str, document: &str) -> Result<Vec<SchemaError>, String> {
    let schema = parse(schema, "schema")?;
    let document = parse(document, "document")?;
    let validator = jsonschema::options()
        .should_validate_formats(true)
        .build(&schema)
        .map_err(|e| format!("The schema is invalid: {e}"))?;
    Ok(validator
        .iter_errors(&document)
        .map(|error| SchemaError {
            path: error.instance_path().to_string(),
            message: error.to_string(),
        })
        .collect())
}

/// A schema (draft 2020-12) describing `sample`: types, object
/// properties (all present ones required), array items merged across
/// elements, and common string formats.
pub fn generate(sample: &str) -> Result<String, String> {
    let sample = parse(sample, "sample")?;
    let mut schema = Map::new();
    schema.insert(
        "$schema".into(),
        json!("https://json-schema.org/draft/2020-12/schema"),
    );
    if let Value::Object(inferred) = infer(&sample) {
        schema.extend(inferred);
    }
    serde_json::to_string_pretty(&Value::Object(schema)).map_err(|e| e.to_string())
}

fn string_format(text: &str) -> Option<&'static str> {
    if chrono::DateTime::parse_from_rfc3339(text).is_ok() {
        Some("date-time")
    } else if chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d").is_ok() {
        Some("date")
    } else if uuid::Uuid::parse_str(text).is_ok() && text.len() == 36 {
        Some("uuid")
    } else if url::Url::parse(text).is_ok_and(|u| u.has_host()) {
        Some("uri")
    } else if text.split_once('@').is_some_and(|(user, host)| {
        !user.is_empty() && host.contains('.') && !text.contains(char::is_whitespace)
    }) {
        Some("email")
    } else {
        None
    }
}

fn infer(value: &Value) -> Value {
    match value {
        Value::Null => json!({ "type": "null" }),
        Value::Bool(_) => json!({ "type": "boolean" }),
        Value::Number(n) if n.is_i64() || n.is_u64() => json!({ "type": "integer" }),
        Value::Number(_) => json!({ "type": "number" }),
        Value::String(text) => match string_format(text) {
            Some(format) => json!({ "type": "string", "format": format }),
            None => json!({ "type": "string" }),
        },
        Value::Array(items) => {
            let mut schema = json!({ "type": "array" });
            if let Some(merged) = items.iter().map(infer).reduce(merge) {
                schema["items"] = merged;
            }
            schema
        }
        Value::Object(fields) => {
            let properties: Map<String, Value> = fields
                .iter()
                .map(|(key, value)| (key.clone(), infer(value)))
                .collect();
            let required: Vec<Value> = fields.keys().map(|k| json!(k)).collect();
            let mut schema = json!({ "type": "object", "properties": properties });
            if !required.is_empty() {
                schema["required"] = Value::Array(required);
            }
            schema
        }
    }
}

/// The type names a schema allows.
fn types(schema: &Value) -> Vec<String> {
    match &schema["type"] {
        Value::String(t) => vec![t.clone()],
        Value::Array(list) => list
            .iter()
            .filter_map(|t| t.as_str().map(str::to_string))
            .collect(),
        _ => Vec::new(),
    }
}

/// One schema covering both (array elements of different shapes).
fn merge(a: Value, b: Value) -> Value {
    if a == b {
        return a;
    }
    let (ta, tb) = (types(&a), types(&b));
    match (ta.as_slice(), tb.as_slice()) {
        ([x], [y]) if x == "object" && y == "object" => merge_objects(a, b),
        ([x], [y]) if x == "array" && y == "array" => {
            let items = match (a.get("items").cloned(), b.get("items").cloned()) {
                (Some(x), Some(y)) => Some(merge(x, y)),
                (x, y) => x.or(y),
            };
            let mut schema = json!({ "type": "array" });
            if let Some(items) = items {
                schema["items"] = items;
            }
            schema
        }
        // An integer is a number.
        ([x], [y]) if (x == "integer" && y == "number") || (x == "number" && y == "integer") => {
            json!({ "type": "number" })
        }
        // Strings with different formats: a plain string.
        ([x], [y]) if x == "string" && y == "string" => json!({ "type": "string" }),
        _ => {
            let simple = |s: &Value| {
                s.as_object()
                    .is_some_and(|o| o.len() == 1 && o.contains_key("type"))
            };
            if simple(&a) && simple(&b) {
                let mut all = ta;
                for t in tb {
                    if !all.contains(&t) {
                        all.push(t);
                    }
                }
                json!({ "type": all })
            } else {
                let mut options = match a.get("anyOf") {
                    Some(Value::Array(list)) => list.clone(),
                    _ => vec![a],
                };
                if !options.contains(&b) {
                    options.push(b);
                }
                json!({ "anyOf": options })
            }
        }
    }
}

fn merge_objects(a: Value, b: Value) -> Value {
    let props = |v: &Value| v["properties"].as_object().cloned().unwrap_or_default();
    let required = |v: &Value| -> Vec<String> {
        v["required"]
            .as_array()
            .map(|r| {
                r.iter()
                    .filter_map(|k| k.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    let (pa, pb) = (props(&a), props(&b));
    let mut properties = pa;
    for (key, schema) in pb {
        match properties.get_mut(&key) {
            Some(existing) => *existing = merge(existing.take(), schema),
            None => {
                properties.insert(key, schema);
            }
        }
    }
    // Required only when every element has the key.
    let rb = required(&b);
    let both: Vec<Value> = required(&a)
        .into_iter()
        .filter(|k| rb.contains(k))
        .map(Value::String)
        .collect();
    let mut schema = json!({ "type": "object", "properties": properties });
    if !both.is_empty() {
        schema["required"] = Value::Array(both);
    }
    schema
}

#[cfg(test)]
mod tests {
    use super::*;

    const SCHEMA: &str = r#"{
        "$schema": "https://json-schema.org/draft/2020-12/schema",
        "type": "object",
        "properties": {
            "name": { "type": "string", "minLength": 1 },
            "age": { "type": "integer", "minimum": 0 },
            "email": { "type": "string", "format": "email" }
        },
        "required": ["name"]
    }"#;

    #[test]
    fn validates_documents() {
        assert_eq!(
            validate(SCHEMA, r#"{"name": "Ada", "age": 36}"#).unwrap(),
            []
        );
        let errors = validate(SCHEMA, r#"{"age": -1, "email": "nope"}"#).unwrap();
        let paths: Vec<_> = errors.iter().map(|e| e.path.as_str()).collect();
        assert!(paths.contains(&""), "{errors:?}");
        assert!(paths.contains(&"/age"), "{errors:?}");
        assert!(paths.contains(&"/email"), "{errors:?}");
        assert!(errors.iter().any(|e| e.message.contains("name")));
    }

    #[test]
    fn reports_bad_input() {
        assert!(
            validate("{", "{}")
                .unwrap_err()
                .contains("schema is not valid JSON")
        );
        assert!(validate("{}", "[").unwrap_err().contains("document"));
        assert!(
            validate(r#"{"type": 12}"#, "{}")
                .unwrap_err()
                .contains("schema is invalid")
        );
    }

    #[test]
    fn generates_a_schema_that_accepts_its_sample() {
        let sample = r#"{
            "id": "3f2504e0-4f89-41d3-9a0c-0305e82c3301",
            "name": "Ada",
            "score": 1.5,
            "tags": ["a", "b"],
            "created": "2026-11-17T10:00:00Z",
            "items": [{"sku": "x", "qty": 1}, {"sku": "y", "qty": 2.5, "note": null}],
            "mixed": [1, "two"]
        }"#;
        let schema = generate(sample).unwrap();
        let value: Value = serde_json::from_str(&schema).unwrap();
        assert_eq!(value["properties"]["id"]["format"], "uuid");
        assert_eq!(value["properties"]["created"]["format"], "date-time");
        assert_eq!(value["properties"]["score"]["type"], "number");
        assert_eq!(value["properties"]["tags"]["items"]["type"], "string");
        let item = &value["properties"]["items"]["items"];
        assert_eq!(item["properties"]["qty"]["type"], "number");
        assert_eq!(item["required"], json!(["sku", "qty"]));
        assert_eq!(
            value["properties"]["mixed"]["items"]["type"],
            json!(["integer", "string"])
        );
        assert_eq!(validate(&schema, sample).unwrap(), []);
        assert!(!validate(&schema, r#"{"name": 1}"#).unwrap().is_empty());
    }

    #[test]
    fn generates_for_scalars_and_empty_arrays() {
        let schema: Value = serde_json::from_str(&generate("[]").unwrap()).unwrap();
        assert_eq!(schema["type"], "array");
        assert!(schema.get("items").is_none());
        let schema: Value = serde_json::from_str(&generate("\"a@b.io\"").unwrap()).unwrap();
        assert_eq!(schema["format"], "email");
        assert!(generate("nope").is_err());
    }
}

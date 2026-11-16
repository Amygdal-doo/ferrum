//! `$ref` resolution and example values generated from JSON Schemas.

use serde_json::{Map, Value, json};

/// Depth limit when generating nested examples (also stops `$ref` cycles).
const MAX_DEPTH: usize = 6;

/// Follows `$ref`s (local `#/…` pointers only), at most 16 hops.
pub(super) fn resolve<'a>(doc: &'a Value, mut v: &'a Value) -> &'a Value {
    for _ in 0..16 {
        let Some(target) = v
            .get("$ref")
            .and_then(Value::as_str)
            .and_then(|r| r.strip_prefix('#'))
            .and_then(|p| doc.pointer(p))
        else {
            return v;
        };
        v = target;
    }
    v
}

/// The schema's own example (`example`, the first `examples` entry,
/// `default`, the first `enum` value) or one built from its type.
pub(super) fn example(doc: &Value, schema: &Value) -> Value {
    build(doc, schema, 0)
}

fn build(doc: &Value, schema: &Value, depth: usize) -> Value {
    let s = resolve(doc, schema);
    if let Some(v) = s.get("example").or_else(|| s.get("default")) {
        return v.clone();
    }
    if let Some(v) = s
        .get("examples")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        return v.clone();
    }
    if let Some(v) = s
        .get("enum")
        .and_then(Value::as_array)
        .and_then(|a| a.first())
    {
        return v.clone();
    }
    if depth >= MAX_DEPTH {
        return Value::Null;
    }
    if let Some(all) = s.get("allOf").and_then(Value::as_array) {
        let mut merged = Map::new();
        for part in all {
            if let Value::Object(o) = build(doc, part, depth + 1) {
                merged.extend(o);
            }
        }
        return Value::Object(merged);
    }
    if let Some(first) = ["oneOf", "anyOf"]
        .iter()
        .find_map(|k| s.get(*k).and_then(Value::as_array).and_then(|a| a.first()))
    {
        return build(doc, first, depth + 1);
    }
    let ty = match s.get("type") {
        Some(Value::String(t)) => t.as_str(),
        // OpenAPI 3.1: `type: [string, "null"]`.
        Some(Value::Array(types)) => types
            .iter()
            .filter_map(Value::as_str)
            .find(|t| *t != "null")
            .unwrap_or("null"),
        _ if s.get("properties").is_some() => "object",
        _ if s.get("items").is_some() => "array",
        _ => "",
    };
    match ty {
        "object" => {
            let props = s.get("properties").and_then(Value::as_object);
            Value::Object(
                props
                    .into_iter()
                    .flatten()
                    .map(|(k, v)| (k.clone(), build(doc, v, depth + 1)))
                    .collect(),
            )
        }
        "array" => match s.get("items") {
            Some(items) => json!([build(doc, items, depth + 1)]),
            None => json!([]),
        },
        "integer" => json!(0),
        "number" => json!(0.0),
        "boolean" => json!(true),
        "string" => Value::String(
            match s.get("format").and_then(Value::as_str).unwrap_or("") {
                "date" => "2024-01-01",
                "date-time" => "2024-01-01T00:00:00Z",
                "email" => "user@example.com",
                "uuid" => "00000000-0000-0000-0000-000000000000",
                "uri" | "url" => "https://example.com",
                "binary" => "",
                _ => "string",
            }
            .to_owned(),
        ),
        _ => Value::Null,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn examples_from_refs_all_of_and_types() {
        let doc = json!({"components": {"schemas": {
            "Id": {"type": "integer", "example": 7},
            "Pet": {"allOf": [
                {"type": "object", "properties": {"id": {"$ref": "#/components/schemas/Id"}}},
                {"type": "object", "properties": {
                    "name": {"type": "string"},
                    "born": {"type": "string", "format": "date"},
                    "tags": {"type": "array", "items": {"type": "string", "enum": ["a", "b"]}},
                    "kind": {"oneOf": [{"type": "boolean"}, {"type": "string"}]},
                    "owner": {"type": ["object", "null"], "properties": {"email": {"type": "string", "format": "email"}}},
                    "self": {"$ref": "#/components/schemas/Pet"}
                }}
            ]}
        }}});
        let pet = example(&doc, &json!({"$ref": "#/components/schemas/Pet"}));
        assert_eq!(pet["id"], 7);
        assert_eq!(pet["name"], "string");
        assert_eq!(pet["born"], "2024-01-01");
        assert_eq!(pet["tags"], json!(["a"]));
        assert_eq!(pet["kind"], true);
        assert_eq!(pet["owner"]["email"], "user@example.com");
        // The cycle ends at the depth limit.
        assert!(pet["self"]["self"].is_object());
        let mut cur = &pet;
        let mut depth = 0;
        while let Some(next) = cur.get("self").filter(|v| v.is_object()) {
            cur = next;
            depth += 1;
        }
        assert!(depth < 8);
        let looped = json!({"a": {"$ref": "#/a"}});
        assert_eq!(resolve(&looped, &looped["a"]), &json!({"$ref": "#/a"}));
    }
}

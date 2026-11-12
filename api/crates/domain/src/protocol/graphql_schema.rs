//! A compact GraphQL schema read from an introspection response, and field
//! completion for the query editor.

use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GqlArg {
    pub name: String,
    pub type_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GqlField {
    pub name: String,
    /// Rendered type, e.g. `[User!]!`.
    pub type_ref: String,
    /// The named type inside the wrappers, e.g. `User`.
    pub base_type: String,
    pub args: Vec<GqlArg>,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GqlType {
    pub name: String,
    /// `OBJECT`, `INPUT_OBJECT`, `ENUM`, `SCALAR`, `INTERFACE`, `UNION`.
    pub kind: String,
    pub description: String,
    pub fields: Vec<GqlField>,
    pub input_fields: Vec<GqlField>,
    pub enum_values: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct GqlSchema {
    pub query_type: Option<String>,
    pub mutation_type: Option<String>,
    pub subscription_type: Option<String>,
    pub types: Vec<GqlType>,
}

fn s(v: Option<&Value>) -> String {
    v.and_then(Value::as_str).unwrap_or_default().to_owned()
}

fn root_name(schema: &Value, key: &str) -> Option<String> {
    schema
        .get(key)
        .and_then(|t| t.get("name"))
        .and_then(Value::as_str)
        .map(str::to_owned)
}

/// `(rendered, base name)` of an introspection `type` reference.
fn type_ref(t: &Value) -> (String, String) {
    match t.get("kind").and_then(Value::as_str) {
        Some("NON_NULL") => {
            let (inner, base) = type_ref(t.get("ofType").unwrap_or(&Value::Null));
            (format!("{inner}!"), base)
        }
        Some("LIST") => {
            let (inner, base) = type_ref(t.get("ofType").unwrap_or(&Value::Null));
            (format!("[{inner}]"), base)
        }
        _ => {
            let name = s(t.get("name"));
            (name.clone(), name)
        }
    }
}

fn field(f: &Value) -> GqlField {
    let (type_ref_text, base_type) = type_ref(f.get("type").unwrap_or(&Value::Null));
    let args = f
        .get("args")
        .and_then(Value::as_array)
        .map(|args| {
            args.iter()
                .map(|a| GqlArg {
                    name: s(a.get("name")),
                    type_ref: type_ref(a.get("type").unwrap_or(&Value::Null)).0,
                })
                .collect()
        })
        .unwrap_or_default();
    GqlField {
        name: s(f.get("name")),
        type_ref: type_ref_text,
        base_type,
        args,
        description: s(f.get("description")),
    }
}

fn fields(v: Option<&Value>) -> Vec<GqlField> {
    v.and_then(Value::as_array)
        .map(|fs| fs.iter().map(field).collect())
        .unwrap_or_default()
}

/// Reads `{"data": {"__schema": …}}` (or a bare `{"__schema": …}`).
pub fn parse_introspection(body: &[u8]) -> Result<GqlSchema, String> {
    let value: Value =
        serde_json::from_slice(body).map_err(|e| format!("response is not JSON: {e}"))?;
    let schema = value
        .pointer("/data/__schema")
        .or_else(|| value.get("__schema"))
        .ok_or_else(|| {
            let message = value
                .pointer("/errors/0/message")
                .and_then(Value::as_str)
                .unwrap_or("no __schema in the response (is introspection disabled?)");
            message.to_owned()
        })?;
    let types = schema
        .get("types")
        .and_then(Value::as_array)
        .map(|ts| {
            ts.iter()
                .map(|t| GqlType {
                    name: s(t.get("name")),
                    kind: s(t.get("kind")),
                    description: s(t.get("description")),
                    fields: fields(t.get("fields")),
                    input_fields: fields(t.get("inputFields")),
                    enum_values: t
                        .get("enumValues")
                        .and_then(Value::as_array)
                        .map(|vs| vs.iter().map(|v| s(v.get("name"))).collect())
                        .unwrap_or_default(),
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(GqlSchema {
        query_type: root_name(schema, "queryType"),
        mutation_type: root_name(schema, "mutationType"),
        subscription_type: root_name(schema, "subscriptionType"),
        types,
    })
}

impl GqlSchema {
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&GqlType> {
        self.types.iter().find(|t| t.name == name)
    }

    /// Types without the `__` introspection types, roots first, then by name.
    #[must_use]
    pub fn user_types(&self) -> Vec<&GqlType> {
        let roots = [
            self.query_type.as_deref(),
            self.mutation_type.as_deref(),
            self.subscription_type.as_deref(),
        ];
        let mut out: Vec<&GqlType> = self
            .types
            .iter()
            .filter(|t| !t.name.starts_with("__"))
            .collect();
        out.sort_by_key(|t| {
            let root = roots.iter().position(|r| *r == Some(t.name.as_str()));
            (root.unwrap_or(usize::MAX), t.name.to_lowercase())
        });
        out
    }
}

/// The identifier being typed just before `cursor` (byte offset).
#[must_use]
pub fn partial_word(text: &str, cursor: usize) -> &str {
    let cursor = cursor.min(text.len());
    let before = text.get(..cursor).unwrap_or(text);
    let start = before
        .rfind(|c: char| !(c.is_ascii_alphanumeric() || c == '_'))
        .map_or(0, |i| i + 1);
    &before[start..]
}

/// Name of the object type whose selection set contains `cursor`.
#[must_use]
pub fn type_at(schema: &GqlSchema, text: &str, cursor: usize) -> Option<String> {
    let before = text.get(..cursor.min(text.len())).unwrap_or(text);
    let mut stack: Vec<Option<String>> = Vec::new();
    let mut operation = schema.query_type.clone();
    let mut last_word = String::new();
    let mut prev_word = String::new();
    let mut paren = 0usize;
    let mut chars = before.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        match c {
            '#' => {
                while chars.peek().is_some_and(|(_, c)| *c != '\n') {
                    chars.next();
                }
            }
            '"' => {
                while let Some((_, c)) = chars.next() {
                    if c == '\\' {
                        chars.next();
                    } else if c == '"' {
                        break;
                    }
                }
            }
            '(' => paren += 1,
            ')' => paren = paren.saturating_sub(1),
            _ if paren > 0 => {}
            '{' => {
                let ty = if stack.is_empty() {
                    operation.clone()
                } else if prev_word == "on" {
                    Some(last_word.clone())
                } else {
                    let parent = stack.last().cloned().flatten();
                    parent
                        .and_then(|p| schema.find(&p))
                        .and_then(|t| t.fields.iter().find(|f| f.name == last_word))
                        .map(|f| f.base_type.clone())
                };
                stack.push(ty);
                last_word.clear();
                prev_word.clear();
            }
            '}' => {
                stack.pop();
            }
            c if c.is_ascii_alphanumeric() || c == '_' => {
                let start = i;
                let mut end = i + c.len_utf8();
                while let Some((j, c)) = chars.peek().copied() {
                    if c.is_ascii_alphanumeric() || c == '_' {
                        end = j + c.len_utf8();
                        chars.next();
                    } else {
                        break;
                    }
                }
                let word = &before[start..end];
                if stack.is_empty() {
                    match word {
                        "query" => operation.clone_from(&schema.query_type),
                        "mutation" => operation.clone_from(&schema.mutation_type),
                        "subscription" => operation.clone_from(&schema.subscription_type),
                        _ => {}
                    }
                }
                prev_word = std::mem::replace(&mut last_word, word.to_owned());
            }
            _ => {}
        }
    }
    stack.pop().flatten()
}

/// Fields of the type at `cursor` starting with the word being typed
/// (case-insensitive), at most `limit`.
#[must_use]
pub fn complete_fields<'a>(
    schema: &'a GqlSchema,
    text: &str,
    cursor: usize,
    limit: usize,
) -> Vec<&'a GqlField> {
    let Some(ty) = type_at(schema, text, cursor).and_then(|t| schema.find(&t)) else {
        return Vec::new();
    };
    let prefix = partial_word(text, cursor).to_lowercase();
    ty.fields
        .iter()
        .filter(|f| f.name.to_lowercase().starts_with(&prefix))
        .take(limit)
        .collect()
}

#[cfg(test)]
#[path = "graphql_schema_tests.rs"]
mod tests;

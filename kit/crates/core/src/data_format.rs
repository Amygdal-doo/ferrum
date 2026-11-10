//! JSON formatting/validation and JSON ⇄ YAML ⇄ TOML conversion.
//!
//! Conversions go through `serde_json::Value` (with `preserve_order`, so keys
//! keep their input order).

use serde::Serialize;
use serde_json::Value;
use serde_json::ser::{PrettyFormatter, Serializer};
use std::fmt;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DataFormat {
    Json,
    Yaml,
    Toml,
}

impl DataFormat {
    pub const ALL: [DataFormat; 3] = [DataFormat::Json, DataFormat::Yaml, DataFormat::Toml];

    pub fn label(self) -> &'static str {
        match self {
            DataFormat::Json => "JSON",
            DataFormat::Yaml => "YAML",
            DataFormat::Toml => "TOML",
        }
    }
}

/// A parse or conversion error. `line`/`column` are 1-based when known.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataError {
    pub message: String,
    pub line: Option<usize>,
    pub column: Option<usize>,
}

impl DataError {
    fn plain(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            line: None,
            column: None,
        }
    }
}

impl fmt::Display for DataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => {
                write!(f, "Line {line}, column {column}: {}", self.message)
            }
            (Some(line), None) => write!(f, "Line {line}: {}", self.message),
            _ => f.write_str(&self.message),
        }
    }
}

impl std::error::Error for DataError {}

/// Indentation for pretty-printed JSON.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Indent {
    Spaces(u8),
    Tab,
}

impl Indent {
    fn bytes(self) -> Vec<u8> {
        match self {
            Indent::Spaces(n) => vec![b' '; usize::from(n)],
            Indent::Tab => vec![b'\t'],
        }
    }
}

pub fn parse_json(input: &str) -> Result<Value, DataError> {
    if input.trim().is_empty() {
        return Err(DataError::plain("Input is empty"));
    }
    serde_json::from_str(input).map_err(|e| {
        // serde_json reports "... at line X column Y" in Display; strip it since
        // we report the position separately.
        let message = e.to_string();
        let message = match message.rfind(" at line ") {
            Some(index) => message[..index].to_string(),
            None => message,
        };
        DataError {
            message,
            line: Some(e.line()),
            column: Some(e.column()),
        }
    })
}

/// Pretty-prints JSON with the given indentation.
pub fn format_json(input: &str, indent: Indent) -> Result<String, DataError> {
    let value = parse_json(input)?;
    to_pretty_json(&value, indent)
}

fn to_pretty_json(value: &Value, indent: Indent) -> Result<String, DataError> {
    let indent = indent.bytes();
    let mut out = Vec::new();
    let mut serializer =
        Serializer::with_formatter(&mut out, PrettyFormatter::with_indent(&indent));
    value
        .serialize(&mut serializer)
        .map_err(|e| DataError::plain(e.to_string()))?;
    String::from_utf8(out).map_err(|e| DataError::plain(e.to_string()))
}

/// Removes all insignificant whitespace.
pub fn minify_json(input: &str) -> Result<String, DataError> {
    let value = parse_json(input)?;
    serde_json::to_string(&value).map_err(|e| DataError::plain(e.to_string()))
}

/// Parses `input` as `from` and writes it as `to` (JSON is pretty-printed
/// with two spaces).
pub fn convert(input: &str, from: DataFormat, to: DataFormat) -> Result<String, DataError> {
    let value = parse(input, from)?;
    write(&value, to)
}

pub fn parse(input: &str, format: DataFormat) -> Result<Value, DataError> {
    match format {
        DataFormat::Json => parse_json(input),
        DataFormat::Yaml => {
            if input.trim().is_empty() {
                return Err(DataError::plain("Input is empty"));
            }
            serde_yaml_ng::from_str(input).map_err(|e| {
                let location = e.location();
                DataError {
                    message: e.to_string(),
                    line: location.as_ref().map(|l| l.line()),
                    column: location.as_ref().map(|l| l.column()),
                }
            })
        }
        DataFormat::Toml => {
            let table: toml::Table = toml::from_str(input).map_err(|e| {
                let (line, column) = e.span().map(|span| line_column(input, span.start)).unzip();
                DataError {
                    message: e.message().to_string(),
                    line,
                    column,
                }
            })?;
            serde_json::to_value(table).map_err(|e| DataError::plain(e.to_string()))
        }
    }
}

pub fn write(value: &Value, format: DataFormat) -> Result<String, DataError> {
    match format {
        DataFormat::Json => to_pretty_json(value, Indent::Spaces(2)),
        DataFormat::Yaml => {
            serde_yaml_ng::to_string(value).map_err(|e| DataError::plain(e.to_string()))
        }
        DataFormat::Toml => {
            if !value.is_object() {
                return Err(DataError::plain(
                    "TOML needs a table (object) at the top level",
                ));
            }
            if contains_null(value) {
                return Err(DataError::plain(
                    "TOML has no null value; remove or replace nulls",
                ));
            }
            toml::to_string_pretty(value).map_err(|e| DataError::plain(e.to_string()))
        }
    }
}

fn contains_null(value: &Value) -> bool {
    match value {
        Value::Null => true,
        Value::Array(items) => items.iter().any(contains_null),
        Value::Object(map) => map.values().any(contains_null),
        _ => false,
    }
}

/// 1-based line and column (in chars) of byte `offset`.
fn line_column(text: &str, offset: usize) -> (usize, usize) {
    let offset = offset.min(text.len());
    let before = text.get(..offset).unwrap_or(text);
    let line = before.matches('\n').count() + 1;
    let line_start = before.rfind('\n').map_or(0, |i| i + 1);
    let column = before[line_start..].chars().count() + 1;
    (line, column)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_with_spaces_and_tabs() {
        let out = format_json(r#"{"a":1,"b":[true,null]}"#, Indent::Spaces(2)).unwrap();
        assert_eq!(
            out,
            "{\n  \"a\": 1,\n  \"b\": [\n    true,\n    null\n  ]\n}"
        );
        let tabbed = format_json(r#"{"a":1}"#, Indent::Tab).unwrap();
        assert_eq!(tabbed, "{\n\t\"a\": 1\n}");
    }

    #[test]
    fn keeps_key_order() {
        let out = minify_json(r#"{ "z": 1, "a": 2 }"#).unwrap();
        assert_eq!(out, r#"{"z":1,"a":2}"#);
    }

    #[test]
    fn minifies() {
        assert_eq!(minify_json(" [ 1 , 2 ,\n 3 ] ").unwrap(), "[1,2,3]");
    }

    #[test]
    fn reports_error_line_and_column() {
        let err = parse_json("{\n  \"a\": 1,\n  \"b\": }").unwrap_err();
        assert_eq!(err.line, Some(3));
        assert_eq!(err.column, Some(8));
        assert!(!err.message.contains("at line"));
        assert!(err.to_string().starts_with("Line 3, column 8: "));
    }

    #[test]
    fn empty_input_is_an_error() {
        assert_eq!(parse_json("  ").unwrap_err().message, "Input is empty");
        assert!(parse("", DataFormat::Yaml).is_err());
    }

    #[test]
    fn json_to_yaml_and_back() {
        let yaml = convert(
            r#"{"name":"kit","tags":["a","b"]}"#,
            DataFormat::Json,
            DataFormat::Yaml,
        )
        .unwrap();
        assert_eq!(yaml, "name: kit\ntags:\n- a\n- b\n");
        let json = convert(&yaml, DataFormat::Yaml, DataFormat::Json).unwrap();
        assert_eq!(
            json,
            "{\n  \"name\": \"kit\",\n  \"tags\": [\n    \"a\",\n    \"b\"\n  ]\n}"
        );
    }

    #[test]
    fn json_to_toml_and_back() {
        let toml = convert(
            r#"{"title":"x","server":{"port":8080,"tls":false}}"#,
            DataFormat::Json,
            DataFormat::Toml,
        )
        .unwrap();
        assert!(toml.contains("title = \"x\""));
        assert!(toml.contains("[server]"));
        assert!(toml.contains("port = 8080"));
        let back = parse(&toml, DataFormat::Toml).unwrap();
        assert_eq!(back["server"]["port"], 8080);
    }

    #[test]
    fn toml_rejects_non_tables_and_nulls() {
        assert!(convert("[1,2]", DataFormat::Json, DataFormat::Toml).is_err());
        let err = convert(r#"{"a":null}"#, DataFormat::Json, DataFormat::Toml).unwrap_err();
        assert!(err.message.contains("null"));
    }

    #[test]
    fn toml_error_has_position() {
        let err = parse("a = 1\nb = = 2", DataFormat::Toml).unwrap_err();
        assert_eq!(err.line, Some(2));
        assert!(err.column.is_some());
    }

    #[test]
    fn yaml_error_has_position() {
        let err = parse("a: [1, 2\nb: 3", DataFormat::Yaml).unwrap_err();
        assert!(err.line.is_some());
    }

    #[test]
    fn line_column_counts_chars() {
        assert_eq!(line_column("ab\ncdé", 0), (1, 1));
        assert_eq!(line_column("ab\ncdé", 3), (2, 1));
        assert_eq!(line_column("ab\nédx", 6), (2, 3));
    }
}

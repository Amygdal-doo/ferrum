//! RESP replies → grid columns and rows. Scalars become one `value` cell,
//! arrays one row per element, maps (and RESP2 pair arrays such as
//! `HGETALL`) one row per entry; nested values are shown as JSON.

use crate::command::ReplyShape;
use ferrumdata_db_driver_api::documents::DocumentTable;
use ferrumdata_domain::{CellValue, ColumnMeta};
use redis::Value;

/// A reply laid out for the grid.
#[derive(Debug, Clone, PartialEq)]
pub struct ReplyTable {
    pub columns: Vec<ColumnMeta>,
    pub rows: Vec<Vec<CellValue>>,
    pub truncated: bool,
}

/// Bytes as text when they are UTF-8, otherwise as binary.
#[must_use]
pub fn bytes_cell(bytes: Vec<u8>) -> CellValue {
    match String::from_utf8(bytes) {
        Ok(s) => CellValue::Text(s),
        Err(e) => CellValue::Binary(e.into_bytes()),
    }
}

fn is_nested(value: &Value) -> bool {
    matches!(
        value,
        Value::Array(_)
            | Value::Set(_)
            | Value::Map(_)
            | Value::Push { .. }
            | Value::Attribute { .. }
    )
}

/// One cell: scalars typed, nested values as compact JSON.
#[must_use]
pub fn cell(value: Value) -> CellValue {
    match value {
        Value::Nil => CellValue::Null,
        Value::Int(i) => CellValue::Signed(i),
        Value::BulkString(b) => bytes_cell(b),
        Value::SimpleString(s) => CellValue::Text(s),
        Value::Okay => CellValue::Text("OK".into()),
        Value::Double(d) => CellValue::Float(d),
        Value::Boolean(b) => CellValue::Bool(b),
        Value::VerbatimString { text, .. } => CellValue::Text(text),
        Value::BigNumber(n) => CellValue::Decimal(big_number(&n)),
        Value::ServerError(e) => CellValue::Text(format!("(error) {e}")),
        nested => CellValue::Json(to_json(&nested).to_string()),
    }
}

fn big_number(n: &[u8]) -> String {
    String::from_utf8_lossy(n).into_owned()
}

/// JSON rendering of any reply (non-UTF-8 bytes are converted lossily).
#[must_use]
pub fn to_json(value: &Value) -> serde_json::Value {
    use serde_json::Value as J;
    let text = |b: &[u8]| J::String(String::from_utf8_lossy(b).into_owned());
    match value {
        Value::Nil => J::Null,
        Value::Int(i) => J::from(*i),
        Value::BulkString(b) => text(b),
        Value::SimpleString(s) | Value::VerbatimString { text: s, .. } => J::String(s.clone()),
        Value::Okay => J::String("OK".into()),
        Value::Double(d) => {
            serde_json::Number::from_f64(*d).map_or_else(|| J::String(d.to_string()), J::Number)
        }
        Value::Boolean(b) => J::Bool(*b),
        Value::BigNumber(n) => J::String(big_number(n)),
        Value::ServerError(e) => J::String(format!("(error) {e}")),
        Value::Array(items) | Value::Set(items) | Value::Push { data: items, .. } => {
            J::Array(items.iter().map(to_json).collect())
        }
        Value::Attribute { data, .. } => to_json(data),
        Value::Map(entries) => {
            let all_text = entries.iter().all(|(k, _)| {
                matches!(
                    k,
                    Value::BulkString(_) | Value::SimpleString(_) | Value::VerbatimString { .. }
                )
            });
            if all_text {
                J::Object(
                    entries
                        .iter()
                        .map(|(k, v)| {
                            let key = match to_json(k) {
                                J::String(s) => s,
                                other => other.to_string(),
                            };
                            (key, to_json(v))
                        })
                        .collect(),
                )
            } else {
                J::Array(
                    entries
                        .iter()
                        .map(|(k, v)| J::Array(vec![to_json(k), to_json(v)]))
                        .collect(),
                )
            }
        }
        // Reply kinds added by newer client versions.
        other => J::String(format!("{other:?}")),
    }
}

/// Collects documents up to a row limit, counting the rest.
struct Rows {
    table: DocumentTable,
    max: usize,
    total: usize,
}

impl Rows {
    fn new(max: usize) -> Self {
        Self {
            table: DocumentTable::new(),
            max,
            total: 0,
        }
    }

    fn push(&mut self, doc: Vec<(&str, CellValue)>) {
        self.total += 1;
        if self.total <= self.max {
            self.table
                .push(doc.into_iter().map(|(k, v)| (k.to_owned(), v)).collect());
        }
    }
}

/// Columns for a reply without elements, so the grid still shows a header.
fn empty_columns(names: &[&str]) -> Vec<ColumnMeta> {
    names
        .iter()
        .map(|n| ColumnMeta {
            name: (*n).to_owned(),
            db_type: "null".into(),
            logical_type: ferrumdata_domain::LogicalType::Other,
            nullable: None,
            primary_key: false,
            editable: false,
        })
        .collect()
}

/// `INFO` text → (section, field, value), in order.
#[must_use]
pub fn parse_info(text: &str) -> Vec<(String, String, String)> {
    let mut section = String::new();
    let mut out = Vec::new();
    for line in text.lines().map(str::trim).filter(|l| !l.is_empty()) {
        if let Some(name) = line.strip_prefix('#') {
            name.trim().clone_into(&mut section);
        } else if let Some((k, v)) = line.split_once(':') {
            out.push((section.clone(), k.to_owned(), v.to_owned()));
        }
    }
    out
}

fn score_cell(value: Value) -> CellValue {
    match cell(value) {
        CellValue::Text(t) => t
            .parse::<f64>()
            .map_or(CellValue::Text(t), CellValue::Float),
        other => other,
    }
}

/// Lays out a reply for the grid, keeping at most `max_rows` rows.
#[must_use]
pub fn reply_table(value: Value, shape: ReplyShape, max_rows: usize) -> ReplyTable {
    let value = match value {
        Value::Attribute { data, .. } => *data,
        other => other,
    };
    let mut rows = Rows::new(max_rows);
    let names: Vec<&str>;
    match value {
        Value::BulkString(b) if shape == ReplyShape::Info => {
            names = vec!["section", "field", "value"];
            for (section, field, value) in parse_info(&String::from_utf8_lossy(&b)) {
                rows.push(vec![
                    ("section", CellValue::Text(section)),
                    ("field", CellValue::Text(field)),
                    ("value", CellValue::Text(value)),
                ]);
            }
        }
        Value::Map(entries) => {
            let (a, b) = match shape {
                ReplyShape::Pairs(a, b) => (a, b),
                _ => ("field", "value"),
            };
            names = vec![a, b];
            for (k, v) in entries {
                let v = if b == "score" { score_cell(v) } else { cell(v) };
                rows.push(vec![(a, cell(k)), (b, v)]);
            }
        }
        Value::Array(items) | Value::Set(items) | Value::Push { data: items, .. } => match shape {
            ReplyShape::Pairs(a, b) if items.len() % 2 == 0 && !items.iter().any(is_nested) => {
                names = vec![a, b];
                let mut it = items.into_iter();
                while let (Some(k), Some(v)) = (it.next(), it.next()) {
                    let v = if b == "score" { score_cell(v) } else { cell(v) };
                    rows.push(vec![(a, cell(k)), (b, v)]);
                }
            }
            _ => {
                names = vec!["#", "value"];
                for (i, item) in items.into_iter().enumerate() {
                    let n = i64::try_from(i + 1).unwrap_or(i64::MAX);
                    rows.push(vec![("#", CellValue::Signed(n)), ("value", cell(item))]);
                }
            }
        },
        scalar => {
            names = vec!["value"];
            rows.push(vec![("value", cell(scalar))]);
        }
    }
    let truncated = rows.total > rows.max;
    let (columns, rows) = if rows.table.is_empty() {
        (empty_columns(&names), Vec::new())
    } else {
        rows.table.finish(&[])
    };
    ReplyTable {
        columns,
        rows,
        truncated,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::LogicalType;

    fn bulk(s: &str) -> Value {
        Value::BulkString(s.as_bytes().to_vec())
    }

    fn names(t: &ReplyTable) -> Vec<&str> {
        t.columns.iter().map(|c| c.name.as_str()).collect()
    }

    #[test]
    fn scalars_are_one_typed_value_cell() {
        let t = reply_table(Value::Int(7), ReplyShape::Plain, 10);
        assert_eq!(names(&t), ["value"]);
        assert_eq!(t.rows, vec![vec![CellValue::Signed(7)]]);
        assert_eq!(t.columns[0].logical_type, LogicalType::Integer);
        let t = reply_table(Value::Nil, ReplyShape::Plain, 10);
        assert_eq!(t.rows, vec![vec![CellValue::Null]]);
        let t = reply_table(Value::Okay, ReplyShape::Plain, 10);
        assert_eq!(t.rows, vec![vec![CellValue::Text("OK".into())]]);
        let t = reply_table(Value::BulkString(vec![0xff, 0]), ReplyShape::Plain, 10);
        assert_eq!(t.rows, vec![vec![CellValue::Binary(vec![0xff, 0])]]);
    }

    #[test]
    fn arrays_are_numbered_rows_with_nested_values_as_json() {
        let v = Value::Array(vec![
            bulk("a"),
            Value::Nil,
            Value::Array(vec![bulk("x"), Value::Int(1)]),
        ]);
        let t = reply_table(v, ReplyShape::Plain, 10);
        assert_eq!(names(&t), ["#", "value"]);
        assert_eq!(
            t.rows[0],
            vec![CellValue::Signed(1), CellValue::Text("a".into())]
        );
        assert_eq!(t.rows[1][1], CellValue::Null);
        assert_eq!(t.rows[2][1], CellValue::Json(r#"["x",1]"#.into()));
        assert!(!t.truncated);
    }

    #[test]
    fn pair_arrays_and_maps_become_two_columns() {
        let v = Value::Array(vec![bulk("f1"), bulk("v1"), bulk("f2"), bulk("v2")]);
        let t = reply_table(v, ReplyShape::Pairs("field", "value"), 10);
        assert_eq!(names(&t), ["field", "value"]);
        assert_eq!(t.rows.len(), 2);
        let v = Value::Array(vec![bulk("m"), bulk("1.5")]);
        let t = reply_table(v, ReplyShape::Pairs("member", "score"), 10);
        assert_eq!(t.rows[0][1], CellValue::Float(1.5));
        let v = Value::Map(vec![(bulk("k"), Value::Int(1))]);
        let t = reply_table(v, ReplyShape::Plain, 10);
        assert_eq!(names(&t), ["field", "value"]);
        // Odd length: not pairs after all.
        let v = Value::Array(vec![bulk("a")]);
        let t = reply_table(v, ReplyShape::Pairs("field", "value"), 10);
        assert_eq!(names(&t), ["#", "value"]);
    }

    #[test]
    fn empty_arrays_keep_a_header_and_limits_truncate() {
        let t = reply_table(Value::Array(vec![]), ReplyShape::Plain, 10);
        assert_eq!(names(&t), ["#", "value"]);
        assert!(t.rows.is_empty());
        let t = reply_table(Value::Array(vec![Value::Int(1); 5]), ReplyShape::Plain, 3);
        assert_eq!(t.rows.len(), 3);
        assert!(t.truncated);
    }

    #[test]
    fn info_is_split_into_sections() {
        let text = "# Server\r\nredis_version:7.4.1\r\nmode:standalone\r\n\r\n# Memory\r\nused_memory:1024\r\n";
        let t = reply_table(bulk(text), ReplyShape::Info, 10);
        assert_eq!(names(&t), ["section", "field", "value"]);
        assert_eq!(t.rows.len(), 3);
        assert_eq!(t.rows[2][0], CellValue::Text("Memory".into()));
        assert_eq!(t.rows[0][2], CellValue::Text("7.4.1".into()));
    }

    #[test]
    fn json_rendering_of_maps_and_doubles() {
        let v = Value::Map(vec![
            (bulk("a"), Value::Double(1.5)),
            (bulk("b"), Value::Boolean(true)),
        ]);
        assert_eq!(to_json(&v).to_string(), r#"{"a":1.5,"b":true}"#);
        let v = Value::Map(vec![(Value::Int(1), bulk("x"))]);
        assert_eq!(to_json(&v).to_string(), r#"[[1,"x"]]"#);
    }
}

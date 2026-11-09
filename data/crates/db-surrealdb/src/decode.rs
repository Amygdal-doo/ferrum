//! SurrealDB values → documents of engine-neutral cells.
//!
//! Objects become documents (top-level fields as columns); scalar results
//! become a single `value` column. Nested arrays/objects render as JSON, and
//! record ids in SurrealQL syntax (`person:tobie`).

use ferrumdata_db_driver_api::documents::{ABSENT, Document, VALUE_COLUMN};
use ferrumdata_domain::CellValue;
use surrealdb::types::{Number, ToSql, Value};

pub fn document(value: Value) -> Document {
    match value {
        Value::Object(object) => object.into_iter().map(|(k, v)| (k, cell(v))).collect(),
        other => vec![(VALUE_COLUMN.to_owned(), cell(other))],
    }
}

pub fn cell(value: Value) -> CellValue {
    match value {
        // NONE is "no value", distinct from NULL.
        Value::None => CellValue::Unsupported(ABSENT.to_owned()),
        Value::Null => CellValue::Null,
        Value::Bool(b) => CellValue::Bool(b),
        Value::Number(n) => number(n),
        Value::String(s) => CellValue::Text(s),
        Value::Datetime(d) => CellValue::TimestampTz(d.fixed_offset()),
        Value::Uuid(u) => CellValue::Uuid(u.into_inner().to_string()),
        Value::Bytes(b) => CellValue::Binary(b.to_vec()),
        v @ (Value::Array(_) | Value::Object(_) | Value::Set(_) | Value::Geometry(_)) => {
            CellValue::Json(v.into_json_value().to_string())
        }
        // Record ids, durations, ranges, tables, files…: SurrealQL text.
        other => CellValue::Text(other.to_sql()),
    }
}

fn number(n: Number) -> CellValue {
    match n {
        Number::Int(v) => CellValue::Signed(v),
        Number::I8(v) => CellValue::Signed(v.into()),
        Number::I16(v) => CellValue::Signed(v.into()),
        Number::I32(v) => CellValue::Signed(v.into()),
        Number::U8(v) => CellValue::Unsigned(v.into()),
        Number::U16(v) => CellValue::Unsigned(v.into()),
        Number::U32(v) => CellValue::Unsigned(v.into()),
        Number::U64(v) => CellValue::Unsigned(v),
        Number::Float(v) => CellValue::Float(v),
        Number::F32(v) => CellValue::Float(v.into()),
        Number::F16(v) => CellValue::Float(v.to_f64()),
        // Exact decimals stay strings.
        Number::Decimal(d) => CellValue::Decimal(d.to_string()),
        Number::BigDecimal(d) => CellValue::Decimal(d.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use surrealdb::types::{Object, RecordId};

    #[test]
    fn scalars_map_to_typed_cells() {
        assert_eq!(cell(Value::Null), CellValue::Null);
        assert_eq!(cell(Value::None), CellValue::Unsupported(ABSENT.into()));
        assert_eq!(cell(Value::Bool(true)), CellValue::Bool(true));
        assert_eq!(cell(Value::Number(Number::Int(-3))), CellValue::Signed(-3));
        assert_eq!(
            cell(Value::Number(Number::Float(1.5))),
            CellValue::Float(1.5)
        );
        assert_eq!(
            cell(Value::String(String::new())),
            CellValue::Text(String::new())
        );
    }

    #[test]
    fn record_ids_render_as_surrealql() {
        let id = Value::RecordId(RecordId::new("person", "tobie"));
        assert_eq!(cell(id), CellValue::Text("person:tobie".into()));
    }

    #[test]
    fn objects_flatten_and_nested_values_become_json() {
        let mut inner = Object::new();
        inner.insert("city", "Oslo".to_owned());
        let mut obj = Object::new();
        obj.insert("name", "Ann".to_owned());
        obj.insert("address", Value::Object(inner));
        let doc = document(Value::Object(obj));
        let address = doc
            .iter()
            .find(|(k, _)| k == "address")
            .map(|(_, v)| v.clone());
        assert_eq!(address, Some(CellValue::Json("{\"city\":\"Oslo\"}".into())));
        assert!(
            doc.iter()
                .any(|(k, v)| k == "name" && *v == CellValue::Text("Ann".into()))
        );
    }

    #[test]
    fn scalar_rows_use_the_value_column() {
        let doc = document(Value::Number(Number::Int(7)));
        assert_eq!(doc, vec![(VALUE_COLUMN.to_owned(), CellValue::Signed(7))]);
    }
}

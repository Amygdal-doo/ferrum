//! BSON documents → documents of engine-neutral cells.
//!
//! Top-level fields become columns; nested documents and arrays are rendered
//! as relaxed Extended JSON (`{"$oid": …}` only appears for nested ids).

use bson::spec::BinarySubtype;
use bson::{Bson, Document as BsonDocument};
use ferrumdata_db_driver_api::documents::{Document, VALUE_COLUMN};
use ferrumdata_domain::CellValue;

pub fn document(doc: BsonDocument) -> Document {
    doc.into_iter().map(|(k, v)| (k, cell(v))).collect()
}

/// A bare value (e.g. one `distinct` result) as a one-column document.
pub fn scalar(value: Bson) -> Document {
    vec![(VALUE_COLUMN.to_owned(), cell(value))]
}

pub fn cell(value: Bson) -> CellValue {
    match value {
        Bson::Null => CellValue::Null,
        Bson::Boolean(b) => CellValue::Bool(b),
        Bson::Int32(i) => CellValue::Signed(i.into()),
        Bson::Int64(i) => CellValue::Signed(i),
        Bson::Double(f) => CellValue::Float(f),
        Bson::Decimal128(d) => CellValue::Decimal(d.to_string()),
        Bson::String(s) | Bson::Symbol(s) | Bson::JavaScriptCode(s) => CellValue::Text(s),
        Bson::ObjectId(oid) => CellValue::Text(oid.to_hex()),
        Bson::DateTime(dt) => chrono::DateTime::from_timestamp_millis(dt.timestamp_millis())
            .map_or_else(
                || CellValue::Text(dt.to_string()),
                |t| CellValue::TimestampTz(t.fixed_offset()),
            ),
        Bson::Binary(bin) if bin.subtype == BinarySubtype::Uuid && bin.bytes.len() == 16 => {
            CellValue::Uuid(uuid_string(&bin.bytes))
        }
        Bson::Binary(bin) => CellValue::Binary(bin.bytes),
        v @ (Bson::Document(_) | Bson::Array(_)) => {
            CellValue::Json(v.into_relaxed_extjson().to_string())
        }
        Bson::Undefined => CellValue::Unsupported("undefined".into()),
        Bson::MinKey => CellValue::Unsupported("minKey".into()),
        Bson::MaxKey => CellValue::Unsupported("maxKey".into()),
        // Regex, timestamp, code-with-scope, DBPointer: shell-like text.
        other => CellValue::Text(other.to_string()),
    }
}

fn uuid_string(b: &[u8]) -> String {
    let hex: String = b.iter().map(|x| format!("{x:02x}")).collect();
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use bson::doc;
    use bson::oid::ObjectId;

    #[test]
    fn scalars_map_to_typed_cells() {
        assert_eq!(cell(Bson::Null), CellValue::Null);
        assert_eq!(cell(Bson::Int32(-3)), CellValue::Signed(-3));
        assert_eq!(cell(Bson::Int64(1 << 40)), CellValue::Signed(1 << 40));
        assert_eq!(cell(Bson::Double(1.5)), CellValue::Float(1.5));
        assert_eq!(
            cell(Bson::String(String::new())),
            CellValue::Text(String::new())
        );
        let oid = ObjectId::parse_str("65a1b2c3d4e5f60718293a4b").expect("oid");
        assert_eq!(
            cell(Bson::ObjectId(oid)),
            CellValue::Text("65a1b2c3d4e5f60718293a4b".into())
        );
        let dt = bson::DateTime::from_millis(1_704_164_645_000);
        assert_eq!(
            cell(Bson::DateTime(dt)).display(),
            "2024-01-02T03:04:05+00:00"
        );
        let dec: bson::Decimal128 = "1.10".parse().expect("decimal");
        assert_eq!(
            cell(Bson::Decimal128(dec)),
            CellValue::Decimal("1.10".into())
        );
    }

    #[test]
    fn uuid_binary_is_formatted() {
        let bin = bson::Binary {
            subtype: BinarySubtype::Uuid,
            bytes: (0u8..16).collect(),
        };
        assert_eq!(
            cell(Bson::Binary(bin)),
            CellValue::Uuid("00010203-0405-0607-0809-0a0b0c0d0e0f".into())
        );
    }

    #[test]
    fn documents_flatten_top_level_and_nest_as_json() {
        let d = document(doc! { "_id": 1, "address": { "city": "Oslo" }, "tags": ["a"] });
        assert_eq!(d[0], ("_id".to_owned(), CellValue::Signed(1)));
        assert_eq!(
            d[1],
            (
                "address".to_owned(),
                CellValue::Json("{\"city\":\"Oslo\"}".into())
            )
        );
        assert_eq!(d[2], ("tags".to_owned(), CellValue::Json("[\"a\"]".into())));
        assert_eq!(scalar(Bson::Int32(7))[0].0, VALUE_COLUMN);
    }
}

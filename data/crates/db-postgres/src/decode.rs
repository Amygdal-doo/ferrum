//! PostgreSQL row → engine-neutral cells.

use ferrumdata_domain::{CellValue, ColumnMeta, LogicalType};
use sqlx::postgres::{PgColumn, PgRow, PgTypeKind};
use sqlx::{Column, Row, TypeInfo, ValueRef};

pub fn column_meta(columns: &[PgColumn]) -> Vec<ColumnMeta> {
    columns
        .iter()
        .map(|c| {
            let db_type = c.type_info().name().to_owned();
            ColumnMeta {
                name: c.name().to_owned(),
                logical_type: logical_type(&db_type),
                db_type,
                nullable: None,
                primary_key: false,
                editable: false,
            }
        })
        .collect()
}

pub fn logical_type(type_name: &str) -> LogicalType {
    match type_name {
        "BOOL" => LogicalType::Boolean,
        "INT2" | "INT4" | "INT8" | "OID" => LogicalType::Integer,
        "FLOAT4" | "FLOAT8" => LogicalType::Float,
        "NUMERIC" | "MONEY" => LogicalType::Decimal,
        "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CHAR" | "CITEXT" | "UNKNOWN" => LogicalType::Text,
        "BYTEA" => LogicalType::Binary,
        "DATE" => LogicalType::Date,
        "TIME" => LogicalType::Time,
        "TIMESTAMP" => LogicalType::Timestamp,
        "TIMESTAMPTZ" => LogicalType::TimestampTz,
        "JSON" | "JSONB" => LogicalType::Json,
        "UUID" => LogicalType::Uuid,
        _ => LogicalType::Other,
    }
}

pub fn decode_row(row: &PgRow) -> Vec<CellValue> {
    (0..row.len()).map(|i| decode_cell(row, i)).collect()
}

fn decode_cell(row: &PgRow, i: usize) -> CellValue {
    let Ok(raw) = row.try_get_raw(i) else {
        return CellValue::Unsupported("unreadable".into());
    };
    if raw.is_null() {
        return CellValue::Null;
    }
    let type_info = raw.type_info().into_owned();
    let name = type_info.name();
    let get = |r: &PgRow| -> Result<CellValue, sqlx::Error> {
        Ok(match name {
            "BOOL" => CellValue::Bool(r.try_get(i)?),
            "INT2" => CellValue::Signed(r.try_get::<i16, _>(i)?.into()),
            "INT4" => CellValue::Signed(r.try_get::<i32, _>(i)?.into()),
            "INT8" => CellValue::Signed(r.try_get(i)?),
            "OID" => CellValue::Unsigned(r.try_get::<sqlx::postgres::types::Oid, _>(i)?.0.into()),
            "FLOAT4" => CellValue::Float(r.try_get::<f32, _>(i)?.into()),
            "FLOAT8" => CellValue::Float(r.try_get(i)?),
            "NUMERIC" => numeric(r, i),
            "BYTEA" => CellValue::Binary(r.try_get(i)?),
            "DATE" => CellValue::Date(r.try_get(i)?),
            "TIME" => CellValue::Time(r.try_get(i)?),
            "TIMESTAMP" => CellValue::Timestamp(r.try_get(i)?),
            "TIMESTAMPTZ" => CellValue::TimestampTz(
                r.try_get::<chrono::DateTime<chrono::Utc>, _>(i)?
                    .fixed_offset(),
            ),
            "JSON" | "JSONB" => CellValue::Json(r.try_get::<serde_json::Value, _>(i)?.to_string()),
            "UUID" => CellValue::Uuid(r.try_get::<uuid::Uuid, _>(i)?.to_string()),
            "TEXT" | "VARCHAR" | "BPCHAR" | "NAME" | "CITEXT" | "UNKNOWN" => {
                CellValue::Text(r.try_get(i)?)
            }
            _ => return Ok(fallback(r, i, &type_info)),
        })
    };
    get(row).unwrap_or_else(|_| CellValue::Unsupported(name.to_owned()))
}

fn numeric(row: &PgRow, i: usize) -> CellValue {
    row.try_get_raw(i)
        .ok()
        .and_then(|raw| {
            raw.as_bytes()
                .ok()
                .and_then(crate::numeric::numeric_to_string)
        })
        .map_or_else(
            || CellValue::Unsupported("NUMERIC".into()),
            CellValue::Decimal,
        )
}

/// Enum values arrive as their UTF-8 label; anything else is shown as its type.
fn fallback(row: &PgRow, i: usize, type_info: &sqlx::postgres::PgTypeInfo) -> CellValue {
    if matches!(type_info.kind(), PgTypeKind::Enum(_))
        && let Ok(raw) = row.try_get_raw(i)
        && let Ok(bytes) = raw.as_bytes()
        && let Ok(text) = std::str::from_utf8(bytes)
    {
        return CellValue::Text(text.to_owned());
    }
    CellValue::Unsupported(type_info.name().to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_types() {
        assert_eq!(logical_type("INT8"), LogicalType::Integer);
        assert_eq!(logical_type("TIMESTAMPTZ"), LogicalType::TimestampTz);
        assert_eq!(logical_type("JSONB"), LogicalType::Json);
        assert_eq!(logical_type("INT4[]"), LogicalType::Other);
    }
}

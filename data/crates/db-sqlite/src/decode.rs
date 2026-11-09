//! SQLite row → engine-neutral cells.
//!
//! SQLite is dynamically typed: the declared column type (if any) drives the
//! logical type shown in the header, while each cell is decoded from the
//! storage class of its actual value (INTEGER, REAL, TEXT, BLOB, NULL).

use ferrumdata_domain::{CellValue, ColumnMeta, LogicalType};
use sqlx::sqlite::{SqliteColumn, SqliteRow};
use sqlx::{Column, Row, TypeInfo, ValueRef};

pub fn column_meta(columns: &[SqliteColumn]) -> Vec<ColumnMeta> {
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
        "BOOLEAN" => LogicalType::Boolean,
        "INTEGER" => LogicalType::Integer,
        "REAL" => LogicalType::Float,
        "NUMERIC" => LogicalType::Decimal,
        "TEXT" => LogicalType::Text,
        "BLOB" => LogicalType::Binary,
        // Stored as TEXT/INTEGER/REAL; rendered from the stored value.
        "DATE" => LogicalType::Date,
        "TIME" => LogicalType::Time,
        "DATETIME" => LogicalType::Timestamp,
        _ => LogicalType::Other,
    }
}

/// `declared_bool[i]` marks columns declared `BOOLEAN`, whose 0/1 integers
/// are shown as booleans.
pub fn decode_row(row: &SqliteRow, declared_bool: &[bool]) -> Vec<CellValue> {
    (0..row.len())
        .map(|i| decode_cell(row, i, declared_bool.get(i).copied().unwrap_or(false)))
        .collect()
}

fn decode_cell(row: &SqliteRow, i: usize, as_bool: bool) -> CellValue {
    let Ok(raw) = row.try_get_raw(i) else {
        return CellValue::Unsupported("unreadable".into());
    };
    if raw.is_null() {
        return CellValue::Null;
    }
    let storage = raw.type_info().name().to_owned();
    // Unchecked: the storage class was just read, so the Rust type matches it
    // even when the declared column type says otherwise.
    let result = match storage.as_str() {
        "INTEGER" | "BOOLEAN" => row.try_get_unchecked::<i64, _>(i).map(|v| {
            if as_bool && (v == 0 || v == 1) {
                CellValue::Bool(v == 1)
            } else {
                CellValue::Signed(v)
            }
        }),
        "REAL" => row.try_get_unchecked::<f64, _>(i).map(CellValue::Float),
        "BLOB" => row
            .try_get_unchecked::<Vec<u8>, _>(i)
            .map(CellValue::Binary),
        "TEXT" | "NUMERIC" | "DATE" | "TIME" | "DATETIME" => {
            row.try_get_unchecked::<String, _>(i).map(CellValue::Text)
        }
        _ => return CellValue::Unsupported(storage),
    };
    result.unwrap_or(CellValue::Unsupported(storage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_declared_types() {
        assert_eq!(logical_type("INTEGER"), LogicalType::Integer);
        assert_eq!(logical_type("BOOLEAN"), LogicalType::Boolean);
        assert_eq!(logical_type("DATETIME"), LogicalType::Timestamp);
        assert_eq!(logical_type("NULL"), LogicalType::Other);
    }
}

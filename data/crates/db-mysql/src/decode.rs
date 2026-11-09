//! MySQL row → engine-neutral cells.

use ferrumdata_domain::{CellValue, ColumnMeta, LogicalType};
use sqlx::mysql::{MySqlColumn, MySqlRow};
use sqlx::{Column, Row, TypeInfo, ValueRef};

pub fn column_meta(columns: &[MySqlColumn]) -> Vec<ColumnMeta> {
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
        "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" | "TINYINT UNSIGNED"
        | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED" | "INT UNSIGNED" | "BIGINT UNSIGNED"
        | "YEAR" | "BIT" => LogicalType::Integer,
        "FLOAT" | "DOUBLE" => LogicalType::Float,
        "DECIMAL" => LogicalType::Decimal,
        "CHAR" | "VARCHAR" | "TINYTEXT" | "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM" | "SET" => {
            LogicalType::Text
        }
        "BINARY" | "VARBINARY" | "TINYBLOB" | "BLOB" | "MEDIUMBLOB" | "LONGBLOB" | "GEOMETRY" => {
            LogicalType::Binary
        }
        "DATE" => LogicalType::Date,
        "TIME" => LogicalType::Time,
        "DATETIME" => LogicalType::Timestamp,
        // SQLx sets the session time zone to UTC, so TIMESTAMPs arrive as UTC.
        "TIMESTAMP" => LogicalType::TimestampTz,
        "JSON" => LogicalType::Json,
        _ => LogicalType::Other,
    }
}

pub fn decode_row(row: &MySqlRow) -> Vec<CellValue> {
    (0..row.len()).map(|i| decode_cell(row, i)).collect()
}

fn decode_cell(row: &MySqlRow, i: usize) -> CellValue {
    let Ok(raw) = row.try_get_raw(i) else {
        return CellValue::Unsupported("unreadable".into());
    };
    if raw.is_null() {
        return CellValue::Null;
    }
    let type_info = raw.type_info().into_owned();
    let name = type_info.name();
    // Integers are decoded unchecked into the widest type: SQLx reads the
    // little-endian width it was sent, so TINYINT..BIGINT all fit.
    let get = |r: &MySqlRow| -> Result<CellValue, sqlx::Error> {
        Ok(match name {
            "BOOLEAN" => match r.try_get_unchecked::<i64, _>(i)? {
                v @ (0 | 1) => CellValue::Bool(v == 1),
                v => CellValue::Signed(v),
            },
            "TINYINT" | "SMALLINT" | "MEDIUMINT" | "INT" | "BIGINT" => {
                CellValue::Signed(r.try_get_unchecked(i)?)
            }
            "TINYINT UNSIGNED" | "SMALLINT UNSIGNED" | "MEDIUMINT UNSIGNED" | "INT UNSIGNED"
            | "BIGINT UNSIGNED" | "YEAR" | "BIT" => CellValue::Unsigned(r.try_get_unchecked(i)?),
            "FLOAT" => CellValue::Float(r.try_get::<f32, _>(i)?.into()),
            "DOUBLE" => CellValue::Float(r.try_get(i)?),
            // DECIMAL is sent as its exact decimal text in both protocols.
            "DECIMAL" => CellValue::Decimal(r.try_get_unchecked(i)?),
            "CHAR" | "VARCHAR" | "TINYTEXT" | "TEXT" | "MEDIUMTEXT" | "LONGTEXT" | "ENUM"
            | "SET" => CellValue::Text(r.try_get_unchecked(i)?),
            "BINARY" | "VARBINARY" | "TINYBLOB" | "BLOB" | "MEDIUMBLOB" | "LONGBLOB"
            | "GEOMETRY" => CellValue::Binary(r.try_get_unchecked(i)?),
            "DATE" => CellValue::Date(r.try_get(i)?),
            "TIME" => time(r, i)?,
            "DATETIME" => CellValue::Timestamp(r.try_get(i)?),
            "TIMESTAMP" => CellValue::TimestampTz(
                r.try_get::<chrono::DateTime<chrono::Utc>, _>(i)?
                    .fixed_offset(),
            ),
            "JSON" => CellValue::Json(r.try_get::<serde_json::Value, _>(i)?.to_string()),
            _ => return Ok(unsupported(name)),
        })
    };
    get(row).unwrap_or_else(|_| unsupported(name))
}

/// MySQL TIME is a signed interval (-838:59:59 … 838:59:59), not a time of
/// day: values outside 00:00–24:00 (including negatives) are shown as text.
fn time(row: &MySqlRow, i: usize) -> Result<CellValue, sqlx::Error> {
    let t = row.try_get::<sqlx::mysql::types::MySqlTime, _>(i)?;
    if t.is_valid_time_of_day()
        && let Some(tod) = chrono::NaiveTime::from_hms_micro_opt(
            t.hours(),
            t.minutes().into(),
            t.seconds().into(),
            t.microseconds(),
        )
    {
        return Ok(CellValue::Time(tod));
    }
    // MySQL's own format: `-01:02:03`, `838:59:59.000001`.
    // `MySqlTime::is_negative` is inverted in SQLx 0.9.0; ask the sign directly.
    let sign = if t.sign().is_negative() { "-" } else { "" };
    let mut text = format!(
        "{sign}{:02}:{:02}:{:02}",
        t.hours(),
        t.minutes(),
        t.seconds()
    );
    if t.microseconds() > 0 {
        text.push_str(&format!(".{:06}", t.microseconds()));
    }
    Ok(CellValue::Text(text))
}

fn unsupported(name: &str) -> CellValue {
    CellValue::Unsupported(name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_common_types() {
        assert_eq!(logical_type("BIGINT UNSIGNED"), LogicalType::Integer);
        assert_eq!(logical_type("BOOLEAN"), LogicalType::Boolean);
        assert_eq!(logical_type("DECIMAL"), LogicalType::Decimal);
        assert_eq!(logical_type("TIMESTAMP"), LogicalType::TimestampTz);
        assert_eq!(logical_type("VARBINARY"), LogicalType::Binary);
        assert_eq!(logical_type("JSON"), LogicalType::Json);
        assert_eq!(logical_type("NULL"), LogicalType::Other);
    }
}

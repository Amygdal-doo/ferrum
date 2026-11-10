//! Key browser reads: `SCAN` pages with type and TTL, and per-type value
//! views shaped for the results grid.

use crate::reply::{bytes_cell, cell};
use ferrumdata_db_driver_api::documents::DocumentTable;
use ferrumdata_domain::{
    CellValue, ColumnMeta, KeyEntry, KeyKind, KeyScanPage, KeyTtl, KeyValueView, LogicalType,
};
use redis::aio::MultiplexedConnection;
use redis::{RedisResult, Value};
use std::collections::HashSet;

/// `SCAN` round trips per page at most: a sparse `MATCH` can return empty
/// batches for a long time, and the user can always load more.
const MAX_SCAN_ROUNDS: usize = 32;

/// String values above this size are shown truncated (`GETRANGE`).
pub const STRING_PREVIEW_BYTES: u64 = 1024 * 1024;

/// Elements requested per `HSCAN` / `SSCAN` call.
const ELEMENT_SCAN_COUNT: usize = 500;

fn text(value: &Value) -> String {
    match value {
        Value::SimpleString(s) => s.clone(),
        Value::BulkString(b) => String::from_utf8_lossy(b).into_owned(),
        Value::Okay => "OK".into(),
        other => format!("{other:?}"),
    }
}

fn int(value: &Value) -> Option<i64> {
    match value {
        Value::Int(i) => Some(*i),
        Value::BulkString(b) => std::str::from_utf8(b).ok()?.parse().ok(),
        _ => None,
    }
}

/// One page of keys from `cursor`, with `TYPE` and `PTTL` fetched in one pipeline.
pub async fn scan(
    conn: &mut MultiplexedConnection,
    pattern: &str,
    mut cursor: u64,
    page_size: usize,
) -> RedisResult<KeyScanPage> {
    let page_size = page_size.max(1);
    let count = page_size.clamp(100, 1000);
    let mut keys: Vec<Vec<u8>> = Vec::new();
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    for _ in 0..MAX_SCAN_ROUNDS {
        let (next, batch): (u64, Vec<Vec<u8>>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg(pattern)
            .arg("COUNT")
            .arg(count)
            .query_async(conn)
            .await?;
        // SCAN may return a key more than once.
        keys.extend(batch.into_iter().filter(|k| seen.insert(k.clone())));
        cursor = next;
        if cursor == 0 || keys.len() >= page_size {
            break;
        }
    }
    if keys.is_empty() {
        return Ok(KeyScanPage {
            keys: Vec::new(),
            cursor,
        });
    }
    let mut pipe = redis::pipe();
    for key in &keys {
        pipe.cmd("TYPE").arg(key).cmd("PTTL").arg(key);
    }
    let meta: Vec<Value> = pipe.query_async(conn).await?;
    let entries = keys
        .into_iter()
        .zip(meta.chunks(2))
        .map(|(raw, pair)| {
            let (name, binary) = match String::from_utf8(raw) {
                Ok(s) => (s, false),
                Err(e) => (String::from_utf8_lossy(e.as_bytes()).into_owned(), true),
            };
            KeyEntry {
                name,
                kind: pair
                    .first()
                    .map_or(KeyKind::Missing, |v| KeyKind::from_type_name(&text(v))),
                ttl: pair
                    .get(1)
                    .and_then(int)
                    .map_or(KeyTtl::Missing, KeyTtl::from_pttl),
                binary,
            }
        })
        .collect();
    Ok(KeyScanPage {
        keys: entries,
        cursor,
    })
}

fn column(name: &str, db_type: &str, logical_type: LogicalType, key: bool) -> ColumnMeta {
    ColumnMeta {
        name: name.to_owned(),
        db_type: db_type.to_owned(),
        logical_type,
        nullable: Some(false),
        primary_key: key,
        editable: false,
    }
}

/// Value of a string key: pretty JSON when it is a JSON object or array.
/// Returns (cell, text for the editor, is_json).
#[must_use]
pub fn string_value(bytes: Vec<u8>) -> (CellValue, Option<String>, bool) {
    match String::from_utf8(bytes) {
        Ok(s) => match serde_json::from_str::<serde_json::Value>(&s) {
            Ok(json @ (serde_json::Value::Object(_) | serde_json::Value::Array(_))) => {
                let pretty = serde_json::to_string_pretty(&json).unwrap_or(s);
                (CellValue::Json(pretty.clone()), Some(pretty), true)
            }
            _ => (CellValue::Text(s.clone()), Some(s), false),
        },
        Err(e) => (CellValue::Binary(e.into_bytes()), None, false),
    }
}

/// Collects up to `max` unique elements with a `*SCAN` command
/// (`HSCAN`/`SSCAN`); `pairs` for hashes (field, value).
async fn scan_elements(
    conn: &mut MultiplexedConnection,
    command: &str,
    key: &str,
    pairs: bool,
    max: usize,
) -> RedisResult<Vec<(Vec<u8>, Option<Vec<u8>>)>> {
    let mut out = Vec::new();
    let mut seen: HashSet<Vec<u8>> = HashSet::new();
    let mut cursor = 0u64;
    loop {
        let (next, batch): (u64, Vec<Vec<u8>>) = redis::cmd(command)
            .arg(key)
            .arg(cursor)
            .arg("COUNT")
            .arg(ELEMENT_SCAN_COUNT)
            .query_async(conn)
            .await?;
        let mut items = batch.into_iter();
        while let Some(first) = items.next() {
            let second = if pairs { items.next() } else { None };
            if out.len() < max && seen.insert(first.clone()) {
                out.push((first, second));
            }
        }
        cursor = next;
        if cursor == 0 || out.len() >= max {
            return Ok(out);
        }
    }
}

/// Type, TTL, memory and up to `max_items` elements of `key`.
pub async fn inspect(
    conn: &mut MultiplexedConnection,
    key: &str,
    max_items: usize,
) -> RedisResult<KeyValueView> {
    let max_items = max_items.max(1);
    let (kind, pttl): (String, i64) = redis::pipe()
        .cmd("TYPE")
        .arg(key)
        .cmd("PTTL")
        .arg(key)
        .query_async(conn)
        .await?;
    let kind = KeyKind::from_type_name(&kind);
    // Disabled by ACL/rename on some servers, or missing (pre-4.0): not an error.
    let memory_bytes = redis::cmd("MEMORY")
        .arg("USAGE")
        .arg(key)
        .query_async::<Option<u64>>(conn)
        .await
        .ok()
        .flatten();
    let mut view = KeyValueView {
        key: key.to_owned(),
        kind: kind.clone(),
        ttl: KeyTtl::from_pttl(pttl),
        memory_bytes,
        length: None,
        text: None,
        is_json: false,
        columns: Vec::new(),
        rows: Vec::new(),
        truncated: false,
    };
    let limit = i64::try_from(max_items).unwrap_or(i64::MAX);
    let len_of = |n: u64| usize::try_from(n).unwrap_or(usize::MAX);
    match kind {
        KeyKind::String => {
            let len: u64 = redis::cmd("STRLEN").arg(key).query_async(conn).await?;
            let bytes: Vec<u8> = if len > STRING_PREVIEW_BYTES {
                view.truncated = true;
                redis::cmd("GETRANGE")
                    .arg(key)
                    .arg(0)
                    .arg(STRING_PREVIEW_BYTES - 1)
                    .query_async(conn)
                    .await?
            } else {
                redis::cmd("GET").arg(key).query_async(conn).await?
            };
            let (value, text, is_json) = if view.truncated {
                // A cut JSON document would not parse: show the raw text.
                match bytes_cell(bytes) {
                    CellValue::Text(t) => (CellValue::Text(t.clone()), Some(t), false),
                    other => (other, None, false),
                }
            } else {
                string_value(bytes)
            };
            let ty = if is_json {
                LogicalType::Json
            } else if matches!(value, CellValue::Binary(_)) {
                LogicalType::Binary
            } else {
                LogicalType::Text
            };
            view.length = Some(len);
            view.text = text;
            view.is_json = is_json;
            view.columns = vec![column("value", "string", ty, false)];
            view.rows = vec![vec![value]];
        }
        KeyKind::Hash => {
            let len: u64 = redis::cmd("HLEN").arg(key).query_async(conn).await?;
            let fields = scan_elements(conn, "HSCAN", key, true, max_items).await?;
            view.length = Some(len);
            view.truncated = fields.len() < len_of(len);
            view.columns = vec![
                column("field", "string", LogicalType::Text, true),
                column("value", "string", LogicalType::Text, false),
            ];
            view.rows = fields
                .into_iter()
                .map(|(f, v)| vec![bytes_cell(f), v.map_or(CellValue::Null, bytes_cell)])
                .collect();
        }
        KeyKind::List => {
            let len: u64 = redis::cmd("LLEN").arg(key).query_async(conn).await?;
            let items: Vec<Vec<u8>> = redis::cmd("LRANGE")
                .arg(key)
                .arg(0)
                .arg(limit - 1)
                .query_async(conn)
                .await?;
            view.length = Some(len);
            view.truncated = items.len() < len_of(len);
            view.columns = vec![
                column("index", "integer", LogicalType::Integer, true),
                column("value", "string", LogicalType::Text, false),
            ];
            view.rows = items
                .into_iter()
                .enumerate()
                .map(|(i, v)| {
                    let i = i64::try_from(i).unwrap_or(i64::MAX);
                    vec![CellValue::Signed(i), bytes_cell(v)]
                })
                .collect();
        }
        KeyKind::Set => {
            let len: u64 = redis::cmd("SCARD").arg(key).query_async(conn).await?;
            let members = scan_elements(conn, "SSCAN", key, false, max_items).await?;
            view.length = Some(len);
            view.truncated = members.len() < len_of(len);
            view.columns = vec![column("member", "string", LogicalType::Text, true)];
            view.rows = members
                .into_iter()
                .map(|(m, _)| vec![bytes_cell(m)])
                .collect();
        }
        KeyKind::SortedSet => {
            let len: u64 = redis::cmd("ZCARD").arg(key).query_async(conn).await?;
            let pairs: Vec<(Vec<u8>, f64)> = redis::cmd("ZRANGE")
                .arg(key)
                .arg(0)
                .arg(limit - 1)
                .arg("WITHSCORES")
                .query_async(conn)
                .await?;
            view.length = Some(len);
            view.truncated = pairs.len() < len_of(len);
            view.columns = vec![
                column("member", "string", LogicalType::Text, true),
                column("score", "double", LogicalType::Float, false),
            ];
            view.rows = pairs
                .into_iter()
                .map(|(m, s)| vec![bytes_cell(m), CellValue::Float(s)])
                .collect();
        }
        KeyKind::Stream => {
            let len: u64 = redis::cmd("XLEN").arg(key).query_async(conn).await?;
            let entries: Value = redis::cmd("XRANGE")
                .arg(key)
                .arg("-")
                .arg("+")
                .arg("COUNT")
                .arg(max_items)
                .query_async(conn)
                .await?;
            let (columns, rows, fetched) = stream_table(entries);
            view.length = Some(len);
            view.truncated = fetched < len_of(len);
            view.columns = columns;
            view.rows = rows;
        }
        KeyKind::Missing | KeyKind::Other(_) => {}
    }
    Ok(view)
}

/// `XRANGE` reply → one row per entry: `id` then one column per field
/// (union of every entry's fields, `<absent>` where an entry lacks one).
#[must_use]
pub fn stream_table(reply: Value) -> (Vec<ColumnMeta>, Vec<Vec<CellValue>>, usize) {
    let Value::Array(entries) = reply else {
        return (Vec::new(), Vec::new(), 0);
    };
    let mut table = DocumentTable::new();
    for entry in entries {
        let Value::Array(parts) = entry else { continue };
        let mut parts = parts.into_iter();
        let id = parts.next().map(|v| text(&v)).unwrap_or_default();
        let mut doc = vec![("id".to_owned(), CellValue::Text(id))];
        if let Some(Value::Array(fields)) = parts.next() {
            let mut fields = fields.into_iter();
            while let (Some(f), Some(v)) = (fields.next(), fields.next()) {
                let name = text(&f);
                // `id` is taken by the entry id.
                let name = if name == "id" {
                    "id (field)".to_owned()
                } else {
                    name
                };
                doc.push((name, cell(v)));
            }
        }
        table.push(doc);
    }
    let fetched = table.len();
    let (columns, rows) = table.finish(&["id"]);
    (columns, rows, fetched)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_db_driver_api::documents::ABSENT;

    fn bulk(s: &str) -> Value {
        Value::BulkString(s.as_bytes().to_vec())
    }

    #[test]
    fn json_strings_are_pretty_printed_other_text_is_kept() {
        let (cell, text, json) = string_value(br#"{"a":[1,2]}"#.to_vec());
        assert!(json);
        assert_eq!(
            text.as_deref(),
            Some("{\n  \"a\": [\n    1,\n    2\n  ]\n}")
        );
        assert!(matches!(cell, CellValue::Json(_)));
        // Scalars that happen to parse as JSON stay plain text.
        let (cell, text, json) = string_value(b"42".to_vec());
        assert!(!json);
        assert_eq!(cell, CellValue::Text("42".into()));
        assert_eq!(text.as_deref(), Some("42"));
        let (cell, text, _) = string_value(vec![0xff, 0x00]);
        assert_eq!(cell, CellValue::Binary(vec![0xff, 0x00]));
        assert_eq!(text, None);
    }

    #[test]
    fn stream_entries_become_rows_with_the_union_of_fields() {
        let entry = |id: &str, fields: &[&str]| {
            Value::Array(vec![
                bulk(id),
                Value::Array(fields.iter().map(|f| bulk(f)).collect()),
            ])
        };
        let reply = Value::Array(vec![
            entry("1-0", &["temp", "20", "unit", "C"]),
            entry("2-0", &["temp", "21", "id", "x"]),
        ]);
        let (cols, rows, n) = stream_table(reply);
        let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["id", "temp", "unit", "id (field)"]);
        assert_eq!(n, 2);
        assert_eq!(rows[1][2], CellValue::Unsupported(ABSENT.into()));
        assert_eq!(rows[1][3], CellValue::Text("x".into()));
        assert_eq!(stream_table(Value::Nil).2, 0);
    }
}

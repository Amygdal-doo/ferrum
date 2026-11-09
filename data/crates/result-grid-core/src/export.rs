//! Serialising a result set (or a selection of it) to CSV, JSON, Markdown and
//! TSV. Values are written in full: no display truncation, binary as complete
//! `0x…` hex.
//!
//! - CSV (RFC 4180, CRLF): a header row; fields containing a comma, quote, CR
//!   or LF are quoted with `""` escaping. `NULL` is an empty unquoted field,
//!   an empty string is `""`, so the two stay distinguishable.
//! - JSON: an array of objects keyed by column name (duplicate names get a
//!   `_2`, `_3`… suffix). Numbers stay numbers (decimals verbatim, so no
//!   precision is lost), JSON columns are embedded as JSON, `NULL` is `null`,
//!   and a field absent from a document is omitted.
//! - Markdown: a GitHub table; `|` is escaped and line breaks become `<br>`.
//! - TSV: values only (no header), for pasting into spreadsheets.

use crate::sort::ABSENT_TYPE;
use ferrumdata_domain::{CellValue, ColumnMeta};
use std::borrow::Cow;
use std::io::{self, Write};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Json,
    Markdown,
    Tsv,
}

impl ExportFormat {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Csv => "CSV",
            Self::Json => "JSON",
            Self::Markdown => "Markdown",
            Self::Tsv => "TSV",
        }
    }

    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Json => "json",
            Self::Markdown => "md",
            Self::Tsv => "tsv",
        }
    }
}

/// Writes columns `cols` (indices into `columns`) of `rows` in `format`.
pub fn write_export<'a, W: Write>(
    format: ExportFormat,
    out: &mut W,
    columns: &[ColumnMeta],
    cols: &[usize],
    rows: impl Iterator<Item = &'a [CellValue]>,
) -> io::Result<()> {
    let names: Vec<&str> = cols
        .iter()
        .map(|&c| columns.get(c).map_or("", |m| m.name.as_str()))
        .collect();
    let null = CellValue::Null;
    let pick = |row: &'a [CellValue]| -> Vec<&CellValue> {
        cols.iter().map(|&c| row.get(c).unwrap_or(&null)).collect()
    };
    match format {
        ExportFormat::Csv => {
            write_delimited(out, ',', "\r\n", &names, |cell| csv_field(cell, ','))?;
            for row in rows {
                let cells = pick(row);
                write_record(out, ',', "\r\n", cells.iter().map(|c| csv_field(c, ',')))?;
            }
        }
        ExportFormat::Tsv => {
            for row in rows {
                let cells = pick(row);
                write_record(out, '\t', "\n", cells.iter().map(|c| tsv_field(c)))?;
            }
        }
        ExportFormat::Markdown => {
            let header: Vec<String> = names.iter().map(|n| markdown_text(n)).collect();
            writeln!(out, "| {} |", header.join(" | "))?;
            let align: Vec<&str> = cols
                .iter()
                .map(|&c| {
                    let numeric = columns.get(c).is_some_and(|m| m.logical_type.is_numeric());
                    if numeric { "---:" } else { "---" }
                })
                .collect();
            writeln!(out, "| {} |", align.join(" | "))?;
            for row in rows {
                let cells: Vec<String> = pick(row)
                    .into_iter()
                    .map(|c| markdown_text(&cell_text(c)))
                    .collect();
                writeln!(out, "| {} |", cells.join(" | "))?;
            }
        }
        ExportFormat::Json => {
            let keys: Vec<String> = unique_keys(&names).iter().map(|k| json_string(k)).collect();
            let mut first = true;
            out.write_all(b"[")?;
            for row in rows {
                out.write_all(if first { b"\n  {" } else { b",\n  {" })?;
                first = false;
                let mut first_field = true;
                for (key, cell) in keys.iter().zip(pick(row)) {
                    let Some(value) = json_value(cell) else {
                        continue;
                    };
                    if !first_field {
                        out.write_all(b", ")?;
                    }
                    first_field = false;
                    write!(out, "{key}: {value}")?;
                }
                out.write_all(b"}")?;
            }
            out.write_all(if first { b"]\n" } else { b"\n]\n" })?;
        }
    }
    Ok(())
}

/// [`write_export`] into a string (clipboard).
#[must_use]
pub fn export_string<'a>(
    format: ExportFormat,
    columns: &[ColumnMeta],
    cols: &[usize],
    rows: impl Iterator<Item = &'a [CellValue]>,
) -> String {
    let mut out = Vec::new();
    // Writing to a Vec cannot fail.
    let _ = write_export(format, &mut out, columns, cols, rows);
    String::from_utf8_lossy(&out).into_owned()
}

/// Full, untruncated text of a cell as shown in the grid (`NULL` for null).
#[must_use]
pub fn cell_text(cell: &CellValue) -> Cow<'_, str> {
    match cell {
        CellValue::Decimal(s) | CellValue::Text(s) | CellValue::Json(s) | CellValue::Uuid(s) => {
            Cow::Borrowed(s)
        }
        CellValue::Binary(bytes) => Cow::Owned(hex(bytes)),
        CellValue::Unsupported(ty) => Cow::Owned(format!("<{ty}>")),
        CellValue::TimestampTz(ts) => Cow::Owned(ts.to_rfc3339()),
        other => Cow::Owned(other.display()),
    }
}

fn hex(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(2 + bytes.len() * 2);
    s.push_str("0x");
    for b in bytes {
        s.push_str(&format!("{b:02x}"));
    }
    s
}

fn write_delimited<W: Write>(
    out: &mut W,
    delimiter: char,
    eol: &str,
    names: &[&str],
    field: impl Fn(&CellValue) -> String,
) -> io::Result<()> {
    let fields = names
        .iter()
        .map(|n| field(&CellValue::Text((*n).to_owned())));
    write_record(out, delimiter, eol, fields)
}

fn write_record<W: Write>(
    out: &mut W,
    delimiter: char,
    eol: &str,
    fields: impl Iterator<Item = String>,
) -> io::Result<()> {
    let mut line = fields.collect::<Vec<_>>().join(&delimiter.to_string());
    line.push_str(eol);
    out.write_all(line.as_bytes())
}

/// Quotes when needed; `NULL` is empty and unquoted, `''` is `""`.
fn csv_field(cell: &CellValue, delimiter: char) -> String {
    if cell.is_null() {
        return String::new();
    }
    let text = cell_text(cell);
    if text.is_empty() {
        return "\"\"".into();
    }
    quote_if_needed(&text, delimiter)
}

fn tsv_field(cell: &CellValue) -> String {
    quote_if_needed(&cell_text(cell), '\t')
}

fn quote_if_needed(text: &str, delimiter: char) -> String {
    if text.contains([delimiter, '"', '\n', '\r']) {
        format!("\"{}\"", text.replace('"', "\"\""))
    } else {
        text.to_owned()
    }
}

fn markdown_text(text: &str) -> String {
    text.replace('|', "\\|")
        .replace("\r\n", "<br>")
        .replace(['\n', '\r'], "<br>")
}

/// JSON text of a cell; `None` for a field absent from the document.
fn json_value(cell: &CellValue) -> Option<String> {
    use CellValue as C;
    Some(match cell {
        C::Null => "null".into(),
        C::Bool(b) => b.to_string(),
        C::Signed(v) => v.to_string(),
        C::Unsigned(v) => v.to_string(),
        C::Float(v) if v.is_finite() => v.to_string(),
        C::Float(v) => json_string(&v.to_string()),
        C::Decimal(s) if is_json_number(s) => s.clone(),
        C::Json(s) => serde_json::from_str::<serde_json::Value>(s)
            .ok()
            .and_then(|v| serde_json::to_string(&v).ok())
            .unwrap_or_else(|| json_string(s)),
        C::Timestamp(ts) => json_string(&ts.format("%Y-%m-%dT%H:%M:%S%.f").to_string()),
        C::Unsupported(ty) if ty == ABSENT_TYPE => return None,
        other => json_string(&cell_text(other)),
    })
}

fn json_string(s: &str) -> String {
    serde_json::to_string(s).unwrap_or_else(|_| "\"\"".into())
}

/// RFC 8259 number grammar: `-?(0|[1-9]\d*)(\.\d+)?([eE][+-]?\d+)?`.
fn is_json_number(s: &str) -> bool {
    let b = s.as_bytes();
    let mut i = usize::from(b.first() == Some(&b'-'));
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i - start
    };
    let int_start = i;
    let n = digits(&mut i);
    if n == 0 || (n > 1 && b[int_start] == b'0') {
        return false;
    }
    if b.get(i) == Some(&b'.') {
        i += 1;
        if digits(&mut i) == 0 {
            return false;
        }
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1;
        if matches!(b.get(i), Some(b'+' | b'-')) {
            i += 1;
        }
        if digits(&mut i) == 0 {
            return false;
        }
    }
    i == b.len()
}

/// Column names made unique for use as object keys.
fn unique_keys(names: &[&str]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(names.len());
    for name in names {
        let mut key = (*name).to_owned();
        let mut n = 2;
        while out.contains(&key) {
            key = format!("{name}_{n}");
            n += 1;
        }
        out.push(key);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, NaiveDate};
    use ferrumdata_domain::LogicalType;

    fn col(name: &str, ty: LogicalType) -> ColumnMeta {
        ColumnMeta {
            name: name.into(),
            db_type: "t".into(),
            logical_type: ty,
            nullable: None,
            primary_key: false,
            editable: false,
        }
    }

    fn export(format: ExportFormat, columns: &[ColumnMeta], rows: &[Vec<CellValue>]) -> String {
        let cols: Vec<usize> = (0..columns.len()).collect();
        export_string(format, columns, &cols, rows.iter().map(Vec::as_slice))
    }

    fn text(s: &str) -> CellValue {
        CellValue::Text(s.into())
    }

    #[test]
    fn csv_quotes_escapes_and_keeps_null_distinct_from_empty() {
        let columns = [
            col("id", LogicalType::Integer),
            col("note, \"quoted\"", LogicalType::Text),
        ];
        let rows = vec![
            vec![CellValue::Signed(1), text("say \"hi\", then\nleave")],
            vec![CellValue::Signed(2), CellValue::Null],
            vec![CellValue::Signed(3), text("")],
            vec![CellValue::Signed(4), text("plain ünïcode")],
            vec![CellValue::Signed(5), text("cr\ronly")],
        ];
        assert_eq!(
            export(ExportFormat::Csv, &columns, &rows),
            "id,\"note, \"\"quoted\"\"\"\r\n\
             1,\"say \"\"hi\"\", then\nleave\"\r\n\
             2,\r\n\
             3,\"\"\r\n\
             4,plain ünïcode\r\n\
             5,\"cr\ronly\"\r\n"
        );
    }

    #[test]
    fn binary_and_long_values_are_written_in_full() {
        let columns = [col("b", LogicalType::Binary), col("t", LogicalType::Text)];
        let long = "x".repeat(1000);
        let bytes: Vec<u8> = (0u8..=40).collect();
        let rows = vec![vec![CellValue::Binary(bytes.clone()), text(&long)]];
        let csv = export(ExportFormat::Csv, &columns, &rows);
        let expected_hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
        assert!(
            csv.contains(&format!("0x{expected_hex},{long}\r\n")),
            "{csv}"
        );
        let json = export(ExportFormat::Json, &columns, &rows);
        assert!(
            json.contains(&format!("\"b\": \"0x{expected_hex}\"")),
            "{json}"
        );
        assert_eq!(cell_text(&CellValue::Binary(vec![])), "0x");
    }

    #[test]
    fn json_keeps_types_and_escapes_strings() {
        let columns = [
            col("i", LogicalType::Integer),
            col("d", LogicalType::Decimal),
            col("f", LogicalType::Float),
            col("s", LogicalType::Text),
            col("j", LogicalType::Json),
            col("n", LogicalType::Text),
            col("b", LogicalType::Boolean),
        ];
        let rows = vec![
            vec![
                CellValue::Signed(-7),
                CellValue::Decimal("12345678901234567890.123456789".into()),
                CellValue::Float(0.5),
                text("quote \" backslash \\ newline \n tab \t bell \u{7}"),
                CellValue::Json(r#"{ "a": [1, 2], "b": null }"#.into()),
                CellValue::Null,
                CellValue::Bool(true),
            ],
            vec![
                CellValue::Unsigned(u64::MAX),
                CellValue::Decimal("NaN".into()),
                CellValue::Float(f64::INFINITY),
                text(""),
                CellValue::Json("not json".into()),
                text("x"),
                CellValue::Bool(false),
            ],
        ];
        let out = export(ExportFormat::Json, &columns, &rows);
        assert_eq!(
            out,
            "[\n  {\"i\": -7, \"d\": 12345678901234567890.123456789, \"f\": 0.5, \
             \"s\": \"quote \\\" backslash \\\\ newline \\n tab \\t bell \\u0007\", \
             \"j\": {\"a\":[1,2],\"b\":null}, \"n\": null, \"b\": true},\n  \
             {\"i\": 18446744073709551615, \"d\": \"NaN\", \"f\": \"inf\", \"s\": \"\", \
             \"j\": \"not json\", \"n\": \"x\", \"b\": false}\n]\n"
        );
        // The output is valid JSON with the same values.
        let parsed: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(
            parsed[0]["s"],
            "quote \" backslash \\ newline \n tab \t bell \u{7}"
        );
        assert_eq!(parsed[0]["n"], serde_json::Value::Null);
    }

    #[test]
    fn json_dedupes_keys_omits_absent_fields_and_handles_empty_results() {
        let columns = [
            col("a", LogicalType::Text),
            col("a", LogicalType::Text),
            col("c", LogicalType::Text),
        ];
        let rows = vec![vec![
            text("1"),
            text("2"),
            CellValue::Unsupported(ABSENT_TYPE.into()),
        ]];
        assert_eq!(
            export(ExportFormat::Json, &columns, &rows),
            "[\n  {\"a\": \"1\", \"a_2\": \"2\"}\n]\n"
        );
        assert_eq!(export(ExportFormat::Json, &columns, &[]), "[]\n");
    }

    #[test]
    fn json_formats_temporal_values_as_iso_8601() {
        let ts = NaiveDate::from_ymd_opt(2024, 2, 29)
            .and_then(|d| d.and_hms_milli_opt(13, 5, 9, 250))
            .expect("ts");
        let tz = DateTime::parse_from_rfc3339("2024-02-29T13:05:09+02:00").expect("tz");
        let columns = [
            col("ts", LogicalType::Timestamp),
            col("tz", LogicalType::TimestampTz),
        ];
        let rows = vec![vec![CellValue::Timestamp(ts), CellValue::TimestampTz(tz)]];
        assert_eq!(
            export(ExportFormat::Json, &columns, &rows),
            "[\n  {\"ts\": \"2024-02-29T13:05:09.250\", \"tz\": \"2024-02-29T13:05:09+02:00\"}\n]\n"
        );
    }

    #[test]
    fn markdown_escapes_pipes_and_line_breaks_and_right_aligns_numbers() {
        let columns = [
            col("n", LogicalType::Integer),
            col("a|b", LogicalType::Text),
        ];
        let rows = vec![
            vec![CellValue::Signed(1), text("x | y\r\nz")],
            vec![CellValue::Signed(2), CellValue::Null],
        ];
        assert_eq!(
            export(ExportFormat::Markdown, &columns, &rows),
            "| n | a\\|b |\n| ---: | --- |\n| 1 | x \\| y<br>z |\n| 2 | NULL |\n"
        );
    }

    #[test]
    fn tsv_has_no_header_and_quotes_tabs_and_newlines() {
        let columns = [col("a", LogicalType::Text), col("b", LogicalType::Text)];
        let rows = vec![
            vec![text("x\ty"), CellValue::Null],
            vec![text("1"), text("2")],
        ];
        assert_eq!(
            export(ExportFormat::Tsv, &columns, &rows),
            "\"x\ty\"\tNULL\n1\t2\n"
        );
    }

    #[test]
    fn column_subset_and_order_are_respected() {
        let columns = [
            col("a", LogicalType::Text),
            col("b", LogicalType::Text),
            col("c", LogicalType::Text),
        ];
        let rows = [vec![text("1"), text("2"), text("3")]];
        let out = export_string(
            ExportFormat::Csv,
            &columns,
            &[2, 0],
            rows.iter().map(Vec::as_slice),
        );
        assert_eq!(out, "c,a\r\n3,1\r\n");
    }

    #[test]
    fn json_number_grammar() {
        for ok in ["0", "-0", "1.5", "-12.30", "1e5", "1E-5", "2.5e+10"] {
            assert!(is_json_number(ok), "{ok}");
        }
        for bad in [
            "", "-", "01", "+1", ".5", "1.", "1e", "NaN", "Infinity", "1 ",
        ] {
            assert!(!is_json_number(bad), "{bad}");
        }
    }
}

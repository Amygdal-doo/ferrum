//! CSV import into an existing table: parsing (RFC 4180 quoting, delimiter
//! detection), mapping CSV columns to table columns, and the INSERT script
//! that `ConnectionService::execute_script` runs in one transaction.

use ferrumdata_db_driver_api::ScriptStatement;
use ferrumdata_domain::{DatabaseEngine, TableRef};

/// Rows per INSERT statement.
const ROWS_PER_INSERT: usize = 100;
const DELIMITERS: [char; 4] = [',', ';', '\t', '|'];

/// Parsed CSV text: every record, the header included if there is one.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CsvData {
    pub delimiter: char,
    pub records: Vec<Vec<String>>,
}

/// The delimiter used most in the first line (outside quotes); `,` if none.
#[must_use]
pub fn detect_delimiter(text: &str) -> char {
    let mut counts = [0usize; DELIMITERS.len()];
    let mut quoted = false;
    for c in text.chars() {
        match c {
            '"' => quoted = !quoted,
            '\n' if !quoted => break,
            c => {
                if let Some(i) = DELIMITERS.iter().position(|d| *d == c).filter(|_| !quoted) {
                    counts[i] += 1;
                }
            }
        }
    }
    counts
        .iter()
        .enumerate()
        .max_by_key(|(i, n)| (**n, usize::MAX - i))
        .filter(|(_, n)| **n > 0)
        .map_or(',', |(i, _)| DELIMITERS[i])
}

/// Parses CSV: `"…"` fields may hold delimiters, newlines and `""` quotes;
/// CRLF and LF line ends; a UTF-8 BOM is dropped; blank lines are skipped.
#[must_use]
pub fn parse_csv(text: &str, delimiter: Option<char>) -> CsvData {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let delimiter = delimiter.unwrap_or_else(|| detect_delimiter(text));
    let mut records = Vec::new();
    let mut record: Vec<String> = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    let mut chars = text.chars().peekable();
    let mut end_record = |record: &mut Vec<String>, field: &mut String| {
        record.push(std::mem::take(field));
        let blank = record.len() == 1 && record[0].is_empty();
        let done = std::mem::take(record);
        if !blank {
            records.push(done);
        }
    };
    while let Some(c) = chars.next() {
        if quoted {
            match c {
                '"' if chars.peek() == Some(&'"') => {
                    field.push('"');
                    chars.next();
                }
                '"' => quoted = false,
                c => field.push(c),
            }
            continue;
        }
        match c {
            '"' if field.is_empty() => quoted = true,
            '\r' if chars.peek() == Some(&'\n') => {}
            '\n' => end_record(&mut record, &mut field),
            c if c == delimiter => record.push(std::mem::take(&mut field)),
            c => field.push(c),
        }
    }
    if !field.is_empty() || !record.is_empty() {
        end_record(&mut record, &mut field);
    }
    CsvData { delimiter, records }
}

/// For each table column, the CSV column with the same name (case and
/// surrounding space ignored), if any.
#[must_use]
pub fn auto_map(table_columns: &[String], headers: &[String]) -> Vec<Option<usize>> {
    table_columns
        .iter()
        .map(|c| {
            headers
                .iter()
                .position(|h| h.trim().eq_ignore_ascii_case(c.trim()))
        })
        .collect()
}

/// How the CSV becomes rows of the table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportPlan {
    /// The first record holds column names, not data.
    pub header: bool,
    /// An empty field inserts NULL instead of `''`.
    pub empty_as_null: bool,
    /// Per table column: the CSV column it takes its value from; unmapped
    /// columns get their default.
    pub mapping: Vec<Option<usize>>,
}

impl ImportPlan {
    /// Data records (the header skipped).
    #[must_use]
    pub fn data<'a>(&self, csv: &'a CsvData) -> &'a [Vec<String>] {
        let skip = usize::from(self.header).min(csv.records.len());
        &csv.records[skip..]
    }

    /// The value for each mapped column of one record, as SQL literals.
    /// `Err` names the 1-based line when the record is too short.
    pub fn row_values(
        &self,
        engine: DatabaseEngine,
        record: &[String],
        line: usize,
    ) -> Result<Vec<String>, String> {
        self.mapping
            .iter()
            .flatten()
            .map(|&i| match record.get(i) {
                Some(v) if v.is_empty() && self.empty_as_null => Ok("NULL".to_owned()),
                Some(v) => Ok(engine.string_literal(v)),
                None => Err(format!(
                    "Line {line} has {} fields; column {} is mapped",
                    record.len(),
                    i + 1
                )),
            })
            .collect()
    }

    /// Multi-row INSERTs into `table`, each expected to insert all its rows.
    pub fn statements(
        &self,
        engine: DatabaseEngine,
        table: &TableRef,
        table_columns: &[String],
        csv: &CsvData,
    ) -> Result<Vec<ScriptStatement>, String> {
        let columns: Vec<String> = self
            .mapping
            .iter()
            .zip(table_columns)
            .filter(|(m, _)| m.is_some())
            .map(|(_, c)| engine.quote_ident(c))
            .collect();
        if columns.is_empty() {
            return Err("Map at least one CSV column to a table column".into());
        }
        let first_line = 1 + usize::from(self.header);
        let data = self.data(csv);
        let mut out = Vec::new();
        for (chunk_index, chunk) in data.chunks(ROWS_PER_INSERT).enumerate() {
            let rows: Vec<String> = chunk
                .iter()
                .enumerate()
                .map(|(i, record)| {
                    let line = first_line + chunk_index * ROWS_PER_INSERT + i;
                    self.row_values(engine, record, line)
                        .map(|v| format!("({})", v.join(", ")))
                })
                .collect::<Result<_, _>>()?;
            out.push(ScriptStatement {
                sql: format!(
                    "INSERT INTO {} ({}) VALUES {}",
                    table.qualified(engine),
                    columns.join(", "),
                    rows.join(", ")
                ),
                expect_rows: Some(chunk.len() as u64),
            });
        }
        if out.is_empty() {
            return Err("The file has no data rows".into());
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn strings(row: &[&str]) -> Vec<String> {
        row.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn parses_quotes_newlines_crlf_and_bom() {
        let csv = parse_csv("\u{feff}id,name\r\n1,\"a, \"\"b\"\"\nc\"\r\n\r\n2,\n", None);
        assert_eq!(csv.delimiter, ',');
        assert_eq!(
            csv.records,
            vec![
                strings(&["id", "name"]),
                strings(&["1", "a, \"b\"\nc"]),
                strings(&["2", ""]),
            ]
        );
        assert_eq!(parse_csv("a;b\n1;2", None).records[1], strings(&["1", "2"]));
        assert_eq!(detect_delimiter("a\tb;c\td"), '\t');
        assert_eq!(detect_delimiter("single"), ',');
    }

    #[test]
    fn maps_by_header_name_and_builds_inserts() {
        let csv = parse_csv("Name,ID,extra\nann,1,x\n,2,y\n", None);
        let columns = strings(&["id", "name", "note"]);
        let mapping = auto_map(&columns, &csv.records[0]);
        assert_eq!(mapping, vec![Some(1), Some(0), None]);
        let plan = ImportPlan {
            header: true,
            empty_as_null: true,
            mapping,
        };
        let table = TableRef {
            schema: None,
            name: "t".into(),
        };
        let script = plan
            .statements(DatabaseEngine::Sqlite, &table, &columns, &csv)
            .expect("script");
        assert_eq!(
            script,
            vec![ScriptStatement {
                sql: r#"INSERT INTO "t" ("id", "name") VALUES ('1', 'ann'), ('2', NULL)"#.into(),
                expect_rows: Some(2),
            }]
        );
    }

    #[test]
    fn short_records_and_empty_mappings_are_refused() {
        let csv = parse_csv("a,b\n1\n", None);
        let table = TableRef {
            schema: None,
            name: "t".into(),
        };
        let columns = strings(&["x", "y"]);
        let plan = ImportPlan {
            header: true,
            empty_as_null: false,
            mapping: vec![Some(0), Some(1)],
        };
        let err = plan
            .statements(DatabaseEngine::Postgres, &table, &columns, &csv)
            .expect_err("short");
        assert!(err.contains("Line 2"), "{err}");
        let none = ImportPlan {
            mapping: vec![None, None],
            ..plan
        };
        assert!(
            none.statements(DatabaseEngine::Postgres, &table, &columns, &csv)
                .is_err()
        );
    }

    #[test]
    fn large_files_are_split_into_batches() {
        let text: String = (0..250).map(|i| format!("{i}\n")).collect();
        let csv = parse_csv(&text, Some(','));
        let plan = ImportPlan {
            header: false,
            empty_as_null: true,
            mapping: vec![Some(0)],
        };
        let table = TableRef {
            schema: None,
            name: "t".into(),
        };
        let script = plan
            .statements(DatabaseEngine::MySql, &table, &strings(&["n"]), &csv)
            .expect("script");
        let counts: Vec<Option<u64>> = script.iter().map(|s| s.expect_rows).collect();
        assert_eq!(counts, vec![Some(100), Some(100), Some(50)]);
    }
}

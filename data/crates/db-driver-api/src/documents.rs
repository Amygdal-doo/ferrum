//! Shared helpers for document engines (SurrealDB, MongoDB): flattening
//! schemaless documents into grid columns and emitting them in batches.
//!
//! Columns are the union of the top-level keys of every fetched document, in
//! first-seen order, so documents are buffered (at most `limits.max_rows`) before
//! `Metadata` is sent. Nested values are rendered by the driver as JSON cells.

use ferrumdata_domain::{CellValue, ColumnMeta, LogicalType, QueryEvent};
use std::collections::HashMap;
use tokio::sync::mpsc;

/// One document's top-level fields, in document order.
pub type Document = Vec<(String, CellValue)>;

/// Type name carried by the cell of a field a document does not have. Distinct
/// from `NULL`, which is a field that exists with a null value.
pub const ABSENT: &str = "absent";

/// Column name used when a result row is a scalar rather than a document.
pub const VALUE_COLUMN: &str = "value";

#[derive(Debug, Default)]
pub struct DocumentTable {
    keys: Vec<String>,
    index: HashMap<String, usize>,
    docs: Vec<Document>,
}

impl DocumentTable {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn push(&mut self, doc: Document) {
        for (key, _) in &doc {
            if !self.index.contains_key(key) {
                self.index.insert(key.clone(), self.keys.len());
                self.keys.push(key.clone());
            }
        }
        self.docs.push(doc);
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.docs.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    /// Column metadata plus aligned rows. Keys listed in `first` (e.g. `_id`,
    /// `id`) are moved to the front when present. `db_type` is the engine
    /// name of the first non-null value seen in the column.
    #[must_use]
    pub fn finish(self, first: &[&str]) -> (Vec<ColumnMeta>, Vec<Vec<CellValue>>) {
        let mut order: Vec<usize> = first
            .iter()
            .filter_map(|k| self.index.get(*k).copied())
            .collect();
        let rest: Vec<usize> = (0..self.keys.len())
            .filter(|i| !order.contains(i))
            .collect();
        order.extend(rest);
        // Position of each key in the output row.
        let mut slot = vec![0usize; self.keys.len()];
        for (out, &key) in order.iter().enumerate() {
            slot[key] = out;
        }

        let mut kinds: Vec<Option<&'static str>> = vec![None; order.len()];
        let rows: Vec<Vec<CellValue>> = self
            .docs
            .into_iter()
            .map(|doc| {
                let mut row = vec![CellValue::Unsupported(ABSENT.to_owned()); order.len()];
                for (key, value) in doc {
                    if let Some(&k) = self.index.get(&key) {
                        let at = slot[k];
                        if kinds[at].is_none() && !value.is_null() {
                            kinds[at] = Some(cell_type_name(&value));
                        }
                        row[at] = value;
                    }
                }
                row
            })
            .collect();

        let columns = order
            .iter()
            .zip(kinds)
            .map(|(&k, kind)| {
                let db_type = kind.unwrap_or("null");
                ColumnMeta {
                    name: self.keys[k].clone(),
                    db_type: db_type.to_owned(),
                    logical_type: logical_type_for(db_type),
                    nullable: None,
                    primary_key: first.first().is_some_and(|f| *f == self.keys[k]),
                    editable: false,
                }
            })
            .collect();
        (columns, rows)
    }
}

fn cell_type_name(cell: &CellValue) -> &'static str {
    match cell {
        CellValue::Null => "null",
        CellValue::Bool(_) => "bool",
        CellValue::Signed(_) | CellValue::Unsigned(_) => "int",
        CellValue::Float(_) => "double",
        CellValue::Decimal(_) => "decimal",
        CellValue::Text(_) => "string",
        CellValue::Binary(_) => "binary",
        CellValue::Date(_) => "date",
        CellValue::Time(_) => "time",
        CellValue::Timestamp(_) => "timestamp",
        CellValue::TimestampTz(_) => "datetime",
        CellValue::Json(_) => "json",
        CellValue::Uuid(_) => "uuid",
        CellValue::Unsupported(_) => "other",
    }
}

fn logical_type_for(name: &str) -> LogicalType {
    match name {
        "bool" => LogicalType::Boolean,
        "int" => LogicalType::Integer,
        "double" => LogicalType::Float,
        "decimal" => LogicalType::Decimal,
        "string" => LogicalType::Text,
        "binary" => LogicalType::Binary,
        "date" => LogicalType::Date,
        "time" => LogicalType::Time,
        "timestamp" => LogicalType::Timestamp,
        "datetime" => LogicalType::TimestampTz,
        "json" => LogicalType::Json,
        "uuid" => LogicalType::Uuid,
        _ => LogicalType::Other,
    }
}

/// Sends `Metadata` then `Rows` in batches of `batch_rows`. Returns `false` if
/// the receiver went away (callers treat that as a cancel).
pub async fn send_table(
    tx: &mpsc::Sender<QueryEvent>,
    columns: Vec<ColumnMeta>,
    rows: Vec<Vec<CellValue>>,
    batch_rows: usize,
) -> bool {
    if tx.send(QueryEvent::Metadata(columns)).await.is_err() {
        return false;
    }
    let batch_rows = batch_rows.max(1);
    let mut rows = rows.into_iter().peekable();
    while rows.peek().is_some() {
        let batch: Vec<_> = rows.by_ref().take(batch_rows).collect();
        if tx.send(QueryEvent::Rows(batch)).await.is_err() {
            return false;
        }
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn doc(fields: &[(&str, CellValue)]) -> Document {
        fields
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect()
    }

    #[test]
    fn union_of_keys_in_first_seen_order_with_absent_cells() {
        let mut t = DocumentTable::new();
        t.push(doc(&[
            ("name", CellValue::Text("a".into())),
            ("_id", CellValue::Signed(1)),
        ]));
        t.push(doc(&[
            ("_id", CellValue::Signed(2)),
            ("age", CellValue::Null),
        ]));
        let (cols, rows) = t.finish(&["_id"]);
        let names: Vec<_> = cols.iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["_id", "name", "age"]);
        assert!(cols[0].primary_key);
        assert_eq!(cols[0].logical_type, LogicalType::Integer);
        // Column type comes from the first non-null value; all-null → "null".
        assert_eq!(cols[2].db_type, "null");
        assert_eq!(rows[0][2], CellValue::Unsupported(ABSENT.into()));
        assert_eq!(rows[1][1], CellValue::Unsupported(ABSENT.into()));
        assert_eq!(rows[1][2], CellValue::Null);
    }

    #[tokio::test]
    async fn batches_rows_after_metadata() {
        let (tx, mut rx) = mpsc::channel(16);
        let rows = vec![vec![CellValue::Signed(1)]; 5];
        assert!(send_table(&tx, vec![], rows, 2).await);
        drop(tx);
        let mut sizes = Vec::new();
        while let Some(e) = rx.recv().await {
            if let QueryEvent::Rows(r) = e {
                sizes.push(r.len());
            }
        }
        assert_eq!(sizes, [2, 2, 1]);
    }
}

//! Staged row edits of a table result and the SQL that applies them. Rows
//! are matched by primary key; every statement expects to touch exactly one
//! row, so a row changed or deleted meanwhile rolls the whole script back.

use ferrumdata_db_driver_api::ScriptStatement;
use ferrumdata_domain::{CellValue, DatabaseEngine, TableInfo, TableKind, TableRef};
use std::collections::{BTreeMap, BTreeSet};

/// A result that can be edited: one table's rows including its whole key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EditTarget {
    pub engine: DatabaseEngine,
    pub table: TableRef,
    /// Result column names, in result order.
    pub columns: Vec<String>,
    /// Result column indexes of the primary key, in key order.
    pub key: Vec<usize>,
}

impl EditTarget {
    /// The target for a result of `info`'s table, or why it is read-only.
    pub fn new(
        engine: DatabaseEngine,
        info: &TableInfo,
        columns: &[String],
    ) -> Result<Self, &'static str> {
        if !matches!(
            engine,
            DatabaseEngine::Postgres | DatabaseEngine::MySql | DatabaseEngine::Sqlite
        ) {
            return Err("Row editing is available for PostgreSQL, MySQL and SQLite tables");
        }
        if !matches!(info.kind, TableKind::Table | TableKind::PartitionedTable) {
            return Err("Views cannot be edited here");
        }
        if info.primary_key.is_empty() {
            return Err("Read-only: the table has no primary key");
        }
        let unique = columns
            .iter()
            .all(|c| columns.iter().filter(|o| *o == c).count() == 1);
        let key: Option<Vec<usize>> = info
            .primary_key
            .iter()
            .map(|k| columns.iter().position(|c| c == k))
            .collect();
        match key {
            Some(key) if unique => Ok(Self {
                engine,
                table: info.table.clone(),
                columns: columns.to_vec(),
                key,
            }),
            _ => Err("Read-only: the result does not include the whole primary key"),
        }
    }
}

/// A value typed into a cell. Text is sent as a string literal, which every
/// engine converts to the column's type (and rejects when it cannot).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NewValue {
    Text(String),
    Null,
}

impl NewValue {
    #[must_use]
    pub fn literal(&self, engine: DatabaseEngine) -> String {
        match self {
            Self::Text(text) => engine.string_literal(text),
            Self::Null => "NULL".to_owned(),
        }
    }
}

/// Changes staged on one result. Existing rows are addressed by their index
/// in the result buffer, new rows by their position in [`Self::new_rows`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RowEdits {
    updates: BTreeMap<usize, BTreeMap<usize, NewValue>>,
    deletes: BTreeSet<usize>,
    /// `None` cells are left to the column default.
    inserts: Vec<Vec<Option<NewValue>>>,
}

impl RowEdits {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.updates.is_empty() && self.deletes.is_empty() && self.inserts.is_empty()
    }

    /// Statements the edits produce: one per changed, deleted or new row.
    #[must_use]
    pub fn change_count(&self) -> usize {
        self.updates
            .keys()
            .filter(|r| !self.deletes.contains(r))
            .count()
            + self.deletes.len()
            + self.inserts.len()
    }

    pub fn set(&mut self, row: usize, col: usize, value: NewValue) {
        self.updates.entry(row).or_default().insert(col, value);
    }

    /// Drops a staged change (the value was set back to the original).
    pub fn revert(&mut self, row: usize, col: usize) {
        if let Some(cols) = self.updates.get_mut(&row) {
            cols.remove(&col);
            if cols.is_empty() {
                self.updates.remove(&row);
            }
        }
    }

    #[must_use]
    pub fn value(&self, row: usize, col: usize) -> Option<&NewValue> {
        self.updates.get(&row)?.get(&col)
    }

    /// Marks a row for deletion, or unmarks it.
    pub fn toggle_delete(&mut self, row: usize) {
        if !self.deletes.remove(&row) {
            self.deletes.insert(row);
        }
    }

    #[must_use]
    pub fn is_deleted(&self, row: usize) -> bool {
        self.deletes.contains(&row)
    }

    /// Appends an empty new row of `columns` cells; returns its index.
    pub fn add_row(&mut self, columns: usize) -> usize {
        self.inserts.push(vec![None; columns]);
        self.inserts.len() - 1
    }

    pub fn set_new(&mut self, index: usize, col: usize, value: Option<NewValue>) {
        if let Some(cell) = self.inserts.get_mut(index).and_then(|r| r.get_mut(col)) {
            *cell = value;
        }
    }

    pub fn remove_new(&mut self, index: usize) {
        if index < self.inserts.len() {
            self.inserts.remove(index);
        }
    }

    #[must_use]
    pub fn new_rows(&self) -> &[Vec<Option<NewValue>>] {
        &self.inserts
    }

    /// Deletes, then updates, then inserts. `row` returns the original
    /// cells of a buffer row. Fails when a key value cannot be written as a
    /// literal (NULL or undecoded), naming the 1-based row.
    pub fn statements<'a>(
        &self,
        target: &EditTarget,
        row: impl Fn(usize) -> Option<&'a [CellValue]>,
    ) -> Result<Vec<ScriptStatement>, String> {
        let engine = target.engine;
        let table = target.table.qualified(engine);
        let ident = |col: usize| engine.quote_ident(&target.columns[col]);
        let key_filter = |index: usize| -> Result<String, String> {
            let cells = row(index).ok_or_else(|| format!("Row {} is gone", index + 1))?;
            let parts: Option<Vec<String>> = target
                .key
                .iter()
                .map(|&k| {
                    let value = cells.get(k).filter(|v| !v.is_null())?;
                    Some(format!("{} = {}", ident(k), engine.literal(value)?))
                })
                .collect();
            parts
                .map(|p| p.join(" AND "))
                .ok_or_else(|| format!("Row {}: its key value cannot be matched", index + 1))
        };
        let one = |sql: String| ScriptStatement {
            sql,
            expect_rows: Some(1),
        };

        let mut out = Vec::with_capacity(self.change_count());
        for &index in &self.deletes {
            out.push(one(format!(
                "DELETE FROM {table} WHERE {}",
                key_filter(index)?
            )));
        }
        for (&index, cols) in &self.updates {
            if self.deletes.contains(&index) || cols.is_empty() {
                continue;
            }
            let set: Vec<String> = cols
                .iter()
                .map(|(&c, v)| format!("{} = {}", ident(c), v.literal(engine)))
                .collect();
            let mut statement = one(format!(
                "UPDATE {table} SET {} WHERE {}",
                set.join(", "),
                key_filter(index)?
            ));
            // MySQL counts rows *changed*: setting a value it already has is 0.
            if engine == DatabaseEngine::MySql {
                statement.expect_rows = None;
            }
            out.push(statement);
        }
        for cells in &self.inserts {
            let given: Vec<(usize, &NewValue)> = cells
                .iter()
                .enumerate()
                .filter_map(|(c, v)| Some((c, v.as_ref()?)))
                .collect();
            let sql = if given.is_empty() {
                match engine {
                    DatabaseEngine::MySql => format!("INSERT INTO {table} () VALUES ()"),
                    _ => format!("INSERT INTO {table} DEFAULT VALUES"),
                }
            } else {
                let names: Vec<String> = given.iter().map(|(c, _)| ident(*c)).collect();
                let values: Vec<String> = given.iter().map(|(_, v)| v.literal(engine)).collect();
                format!(
                    "INSERT INTO {table} ({}) VALUES ({})",
                    names.join(", "),
                    values.join(", ")
                )
            };
            out.push(one(sql));
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(pk: &[&str]) -> TableInfo {
        TableInfo {
            table: TableRef {
                schema: Some("public".into()),
                name: "User".into(),
            },
            kind: TableKind::Table,
            primary_key: pk.iter().map(|s| (*s).to_owned()).collect(),
            estimated_rows: None,
        }
    }

    fn cols(names: &[&str]) -> Vec<String> {
        names.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn only_keyed_tables_with_the_whole_key_are_editable() {
        let pg = DatabaseEngine::Postgres;
        assert!(EditTarget::new(pg, &info(&["id"]), &cols(&["id", "name"])).is_ok());
        assert!(EditTarget::new(pg, &info(&[]), &cols(&["id"])).is_err());
        assert!(EditTarget::new(pg, &info(&["a", "b"]), &cols(&["a", "x"])).is_err());
        assert!(EditTarget::new(pg, &info(&["id"]), &cols(&["id", "id"])).is_err());
        let mut view = info(&["id"]);
        view.kind = TableKind::View;
        assert!(EditTarget::new(pg, &view, &cols(&["id"])).is_err());
        assert!(
            EditTarget::new(DatabaseEngine::MongoDb, &info(&["_id"]), &cols(&["_id"])).is_err()
        );
    }

    #[test]
    fn statements_delete_update_then_insert() {
        let target = EditTarget::new(
            DatabaseEngine::Postgres,
            &info(&["id"]),
            &cols(&["id", "name"]),
        )
        .expect("target");
        let rows = [
            vec![CellValue::Signed(1), CellValue::Text("a".into())],
            vec![CellValue::Signed(2), CellValue::Text("b".into())],
        ];
        let mut edits = RowEdits::default();
        edits.set(0, 1, NewValue::Text("it's".into()));
        edits.set(1, 1, NewValue::Null);
        edits.toggle_delete(1);
        let new = edits.add_row(2);
        edits.set_new(new, 1, Some(NewValue::Text("c".into())));
        edits.add_row(2);
        assert_eq!(edits.change_count(), 4);

        let sql: Vec<String> = edits
            .statements(&target, |i| rows.get(i).map(Vec::as_slice))
            .expect("statements")
            .into_iter()
            .map(|s| {
                assert_eq!(s.expect_rows, Some(1));
                s.sql
            })
            .collect();
        assert_eq!(
            sql,
            vec![
                r#"DELETE FROM "public"."User" WHERE "id" = 2"#,
                r#"UPDATE "public"."User" SET "name" = 'it''s' WHERE "id" = 1"#,
                r#"INSERT INTO "public"."User" ("name") VALUES ('c')"#,
                r#"INSERT INTO "public"."User" DEFAULT VALUES"#,
            ]
        );
    }

    #[test]
    fn mysql_updates_do_not_insist_on_a_changed_row() {
        let target = EditTarget::new(DatabaseEngine::MySql, &info(&["id"]), &cols(&["id", "v"]))
            .expect("target");
        let rows = [vec![CellValue::Text("k\\1".into()), CellValue::Null]];
        let mut edits = RowEdits::default();
        edits.set(0, 1, NewValue::Text("x".into()));
        edits.add_row(2);
        let script = edits
            .statements(&target, |i| rows.get(i).map(Vec::as_slice))
            .expect("statements");
        assert_eq!(
            script[0].sql,
            r"UPDATE `public`.`User` SET `v` = 'x' WHERE `id` = 'k\\1'"
        );
        assert_eq!(script[0].expect_rows, None);
        assert_eq!(script[1].sql, "INSERT INTO `public`.`User` () VALUES ()");
    }

    #[test]
    fn rows_without_a_usable_key_are_refused_and_reverts_drop_changes() {
        let target = EditTarget::new(DatabaseEngine::Sqlite, &info(&["id"]), &cols(&["id"]))
            .expect("target");
        let rows = [vec![CellValue::Null]];
        let mut edits = RowEdits::default();
        edits.toggle_delete(0);
        let err = edits
            .statements(&target, |i| rows.get(i).map(Vec::as_slice))
            .expect_err("null key");
        assert!(err.contains("Row 1"), "{err}");
        edits.toggle_delete(0);
        edits.set(0, 0, NewValue::Text("1".into()));
        edits.revert(0, 0);
        assert!(edits.is_empty());
    }
}

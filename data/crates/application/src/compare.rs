//! Schema compare: what differs between a source and a target schema of the
//! same engine, and the migration script that would make the target match
//! the source. The script is only generated, never run here.
//!
//! Tables match by name (PostgreSQL: schema and name; MySQL schemas are
//! databases and usually differ between environments). Indexes and foreign
//! keys match by name (SQLite foreign keys, which are unnamed, by columns).

use crate::ddl::{
    add_foreign_key, column_def, create_index, create_table, dependency_order, primary_key,
    sequences, table_name,
};
use ferrumdata_domain::{
    ColumnInfo, DatabaseEngine, ForeignKeyInfo, IndexInfo, SchemaSnapshot, TableKind, TableRef,
    TableSchema,
};

/// One difference, described for the list in the UI.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Change {
    TableAdded,
    TableRemoved,
    ColumnAdded(String),
    ColumnRemoved(String),
    /// Column name and what changed (`type int → bigint`, `NOT NULL`, …).
    ColumnChanged(String, Vec<String>),
    PrimaryKeyChanged,
    IndexAdded(String),
    IndexRemoved(String),
    IndexChanged(String),
    ForeignKeyAdded(String),
    ForeignKeyRemoved(String),
    ForeignKeyChanged(String),
}

impl Change {
    /// `+`, `−` or `~` and a short description.
    #[must_use]
    pub fn describe(&self) -> (char, String) {
        match self {
            Self::TableAdded => ('+', "table only in source".into()),
            Self::TableRemoved => ('−', "table only in target".into()),
            Self::ColumnAdded(c) => ('+', format!("column {c}")),
            Self::ColumnRemoved(c) => ('−', format!("column {c}")),
            Self::ColumnChanged(c, what) => ('~', format!("column {c}: {}", what.join(", "))),
            Self::PrimaryKeyChanged => ('~', "primary key".into()),
            Self::IndexAdded(i) => ('+', format!("index {i}")),
            Self::IndexRemoved(i) => ('−', format!("index {i}")),
            Self::IndexChanged(i) => ('~', format!("index {i}")),
            Self::ForeignKeyAdded(f) => ('+', format!("foreign key {f}")),
            Self::ForeignKeyRemoved(f) => ('−', format!("foreign key {f}")),
            Self::ForeignKeyChanged(f) => ('~', format!("foreign key {f}")),
        }
    }
}

/// The differences of one table (source name, or target name if removed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TableDiff {
    pub table: TableRef,
    pub changes: Vec<Change>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SchemaDiff {
    pub tables: Vec<TableDiff>,
    /// Tables present on both sides without differences.
    pub identical: usize,
}

impl SchemaDiff {
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.tables.is_empty()
    }
}

fn base_tables(s: &SchemaSnapshot) -> impl Iterator<Item = &TableSchema> {
    s.tables
        .iter()
        .filter(|t| matches!(t.info.kind, TableKind::Table | TableKind::PartitionedTable))
}

fn same_table(engine: DatabaseEngine, a: &TableRef, b: &TableRef) -> bool {
    a.name == b.name && (engine != DatabaseEngine::Postgres || a.schema == b.schema)
}

fn fk_key(fk: &ForeignKeyInfo) -> String {
    if fk.name.is_empty() {
        format!("({}) → {}", fk.columns.join(", "), fk.ref_table.name)
    } else {
        fk.name.clone()
    }
}

/// Foreign keys are equal when they link the same columns the same way (the
/// referenced table by name: schemas differ between databases).
fn same_fk(a: &ForeignKeyInfo, b: &ForeignKeyInfo) -> bool {
    a.columns == b.columns
        && a.ref_table.name == b.ref_table.name
        && a.ref_columns == b.ref_columns
        && a.on_delete.eq_ignore_ascii_case(&b.on_delete)
        && a.on_update.eq_ignore_ascii_case(&b.on_update)
}

fn same_index(a: &IndexInfo, b: &IndexInfo) -> bool {
    a.columns == b.columns && a.unique == b.unique
}

fn column_changes(source: &ColumnInfo, target: &ColumnInfo) -> Vec<String> {
    let mut what = Vec::new();
    if source.data_type != target.data_type {
        what.push(format!("type {} → {}", target.data_type, source.data_type));
    }
    if source.nullable != target.nullable {
        what.push(if source.nullable { "NULL" } else { "NOT NULL" }.into());
    }
    if source.default != target.default {
        what.push(format!(
            "default {} → {}",
            target.default.as_deref().unwrap_or("none"),
            source.default.as_deref().unwrap_or("none")
        ));
    }
    what
}

/// Differences that turn `target` into `source`.
#[must_use]
pub fn diff(
    engine: DatabaseEngine,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) -> SchemaDiff {
    let mut out = SchemaDiff::default();
    for s in base_tables(source) {
        let Some(t) =
            base_tables(target).find(|t| same_table(engine, &s.info.table, &t.info.table))
        else {
            out.tables.push(TableDiff {
                table: s.info.table.clone(),
                changes: vec![Change::TableAdded],
            });
            continue;
        };
        let changes = table_changes(s, t);
        if changes.is_empty() {
            out.identical += 1;
        } else {
            out.tables.push(TableDiff {
                table: s.info.table.clone(),
                changes,
            });
        }
    }
    for t in base_tables(target) {
        if !base_tables(source).any(|s| same_table(engine, &s.info.table, &t.info.table)) {
            out.tables.push(TableDiff {
                table: t.info.table.clone(),
                changes: vec![Change::TableRemoved],
            });
        }
    }
    out
}

fn table_changes(s: &TableSchema, t: &TableSchema) -> Vec<Change> {
    let (sd, td) = (&s.details, &t.details);
    let mut changes = Vec::new();
    for c in &sd.columns {
        match td.columns.iter().find(|o| o.name == c.name) {
            None => changes.push(Change::ColumnAdded(c.name.clone())),
            Some(o) => {
                let what = column_changes(c, o);
                if !what.is_empty() {
                    changes.push(Change::ColumnChanged(c.name.clone(), what));
                }
            }
        }
    }
    for c in td
        .columns
        .iter()
        .filter(|c| !sd.columns.iter().any(|o| o.name == c.name))
    {
        changes.push(Change::ColumnRemoved(c.name.clone()));
    }
    if primary_key(s) != primary_key(t) {
        changes.push(Change::PrimaryKeyChanged);
    }
    let indexes = |d: &ferrumdata_domain::TableDetails| -> Vec<IndexInfo> {
        d.indexes.iter().filter(|i| !i.primary).cloned().collect()
    };
    let (si, ti) = (indexes(sd), indexes(td));
    for i in &si {
        match ti.iter().find(|o| o.name == i.name) {
            None => changes.push(Change::IndexAdded(i.name.clone())),
            Some(o) if !same_index(i, o) => changes.push(Change::IndexChanged(i.name.clone())),
            Some(_) => {}
        }
    }
    for i in ti.iter().filter(|i| !si.iter().any(|o| o.name == i.name)) {
        changes.push(Change::IndexRemoved(i.name.clone()));
    }
    for f in &sd.foreign_keys {
        match td.foreign_keys.iter().find(|o| fk_key(o) == fk_key(f)) {
            None => changes.push(Change::ForeignKeyAdded(fk_key(f))),
            Some(o) if !same_fk(f, o) => changes.push(Change::ForeignKeyChanged(fk_key(f))),
            Some(_) => {}
        }
    }
    for f in td
        .foreign_keys
        .iter()
        .filter(|f| !sd.foreign_keys.iter().any(|o| fk_key(o) == fk_key(f)))
    {
        changes.push(Change::ForeignKeyRemoved(fk_key(f)));
    }
    changes
}

/// The script that applies `diff` to the target, in the target's dialect.
/// Statements that lose data (DROP) are preceded by a warning comment.
#[must_use]
pub fn migration(
    engine: DatabaseEngine,
    diff: &SchemaDiff,
    source: &SchemaSnapshot,
    target: &SchemaSnapshot,
) -> String {
    let find = |snapshot: &SchemaSnapshot, table: &TableRef| {
        base_tables(snapshot)
            .find(|t| same_table(engine, &t.info.table, table))
            .cloned()
    };
    let mut drops = Vec::new();
    let mut creates = Vec::new();
    let mut alters = Vec::new();
    let mut indexes = Vec::new();
    let mut foreign_keys = Vec::new();

    // New tables, parents first.
    let added: Vec<TableSchema> = diff
        .tables
        .iter()
        .filter(|d| d.changes == [Change::TableAdded])
        .filter_map(|d| find(source, &d.table))
        .collect();
    let added_refs: Vec<&TableSchema> = added.iter().collect();
    for i in dependency_order(&added_refs) {
        let t = added_refs[i];
        creates.extend(sequences(engine, t));
        let inline = engine == DatabaseEngine::Sqlite;
        creates.push(create_table(engine, t, inline));
        for index in t.details.indexes.iter().filter(|x| !x.primary) {
            indexes.push(create_index(engine, &t.info.table, index));
        }
        if !inline {
            for fk in &t.details.foreign_keys {
                foreign_keys.push(add_foreign_key(engine, &t.info.table, fk));
            }
        }
    }

    for d in &diff.tables {
        if d.changes == [Change::TableRemoved] {
            drops.push(format!("-- drops table {} and its data", d.table.name));
            drops.push(format!("DROP TABLE {};", table_name(engine, &d.table)));
            continue;
        }
        let (Some(s), Some(t)) = (find(source, &d.table), find(target, &d.table)) else {
            continue;
        };
        if engine == DatabaseEngine::Sqlite {
            sqlite_alter(&s, &t, &d.changes, &mut alters, &mut indexes);
        } else {
            alter(
                engine,
                &s,
                &t,
                &d.changes,
                &mut drops,
                &mut alters,
                &mut indexes,
                &mut foreign_keys,
            );
        }
    }

    let sections = [
        (
            "Drop changed or removed constraints, indexes and tables",
            drops,
        ),
        ("Create tables", creates),
        ("Alter tables", alters),
        ("Create indexes", indexes),
        ("Add foreign keys", foreign_keys),
    ];
    let mut out = format!(
        "-- FerrumData schema migration ({}): makes the target match the source.\n\
         -- Review before running; nothing has been executed.\n",
        engine.label()
    );
    if diff.is_empty() {
        out.push_str("-- The schemas match: nothing to do.\n");
        return out;
    }
    for (title, statements) in sections.into_iter().filter(|(_, s)| !s.is_empty()) {
        out.push_str(&format!("\n-- {title}\n{}\n", statements.join("\n")));
    }
    out
}

/// PostgreSQL / MySQL: `ALTER TABLE` per change.
#[allow(clippy::too_many_arguments)]
fn alter(
    engine: DatabaseEngine,
    s: &TableSchema,
    t: &TableSchema,
    changes: &[Change],
    drops: &mut Vec<String>,
    alters: &mut Vec<String>,
    indexes: &mut Vec<String>,
    foreign_keys: &mut Vec<String>,
) {
    let name = table_name(engine, &s.info.table);
    let target_name = table_name(engine, &t.info.table);
    let q = |n: &str| engine.quote_ident(n);
    let column =
        |table: &TableSchema, c: &str| table.details.columns.iter().find(|x| x.name == c).cloned();
    for change in changes {
        match change {
            Change::ColumnAdded(c) => {
                if let Some(col) = column(s, c) {
                    alters.push(format!(
                        "ALTER TABLE {name} ADD COLUMN {};",
                        column_def(engine, &col)
                    ));
                }
            }
            Change::ColumnRemoved(c) => {
                alters.push(format!("-- drops column {c} and its data"));
                alters.push(format!("ALTER TABLE {target_name} DROP COLUMN {};", q(c)));
            }
            Change::ColumnChanged(c, _) => {
                let (Some(new), Some(old)) = (column(s, c), column(t, c)) else {
                    continue;
                };
                if engine == DatabaseEngine::MySql {
                    alters.push(format!(
                        "ALTER TABLE {name} MODIFY COLUMN {};",
                        column_def(engine, &new)
                    ));
                    continue;
                }
                let col = q(c);
                if new.data_type != old.data_type {
                    alters.push(format!(
                        "ALTER TABLE {name} ALTER COLUMN {col} TYPE {} USING {col}::{};",
                        new.data_type, new.data_type
                    ));
                }
                if new.nullable != old.nullable {
                    let verb = if new.nullable { "DROP" } else { "SET" };
                    alters.push(format!(
                        "ALTER TABLE {name} ALTER COLUMN {col} {verb} NOT NULL;"
                    ));
                }
                if new.default != old.default {
                    alters.push(match &new.default {
                        Some(d) => {
                            format!("ALTER TABLE {name} ALTER COLUMN {col} SET DEFAULT {d};")
                        }
                        None => format!("ALTER TABLE {name} ALTER COLUMN {col} DROP DEFAULT;"),
                    });
                }
            }
            Change::PrimaryKeyChanged => {
                if !primary_key(t).is_empty() {
                    drops.push(match engine {
                        DatabaseEngine::MySql => {
                            format!("ALTER TABLE {target_name} DROP PRIMARY KEY;")
                        }
                        _ => {
                            let constraint =
                                t.details.indexes.iter().find(|i| i.primary).map_or_else(
                                    || format!("{}_pkey", t.info.table.name),
                                    |i| i.name.clone(),
                                );
                            format!(
                                "ALTER TABLE {target_name} DROP CONSTRAINT {};",
                                q(&constraint)
                            )
                        }
                    });
                }
                let key = primary_key(s);
                if !key.is_empty() {
                    let cols: Vec<String> = key.iter().map(|k| q(k)).collect();
                    alters.push(format!(
                        "ALTER TABLE {name} ADD PRIMARY KEY ({});",
                        cols.join(", ")
                    ));
                }
            }
            Change::IndexAdded(i) | Change::IndexChanged(i) => {
                if matches!(change, Change::IndexChanged(_)) {
                    drops.push(drop_index(engine, &t.info.table, i));
                }
                if let Some(index) = s.details.indexes.iter().find(|x| &x.name == i) {
                    indexes.push(create_index(engine, &s.info.table, index));
                }
            }
            Change::IndexRemoved(i) => drops.push(drop_index(engine, &t.info.table, i)),
            Change::ForeignKeyAdded(f) | Change::ForeignKeyChanged(f) => {
                if matches!(change, Change::ForeignKeyChanged(_)) {
                    drops.push(drop_fk(engine, &target_name, f));
                }
                if let Some(fk) = s.details.foreign_keys.iter().find(|x| &fk_key(x) == f) {
                    foreign_keys.push(add_foreign_key(engine, &s.info.table, fk));
                }
            }
            Change::ForeignKeyRemoved(f) => drops.push(drop_fk(engine, &target_name, f)),
            Change::TableAdded | Change::TableRemoved => {}
        }
    }
}

fn drop_index(engine: DatabaseEngine, table: &TableRef, index: &str) -> String {
    match engine {
        DatabaseEngine::MySql => format!(
            "DROP INDEX {} ON {};",
            engine.quote_ident(index),
            table_name(engine, table)
        ),
        DatabaseEngine::Postgres => {
            let index = TableRef {
                schema: table.schema.clone(),
                name: index.to_owned(),
            };
            format!("DROP INDEX {};", index.qualified(engine))
        }
        _ => format!("DROP INDEX {};", engine.quote_ident(index)),
    }
}

fn drop_fk(engine: DatabaseEngine, table: &str, name: &str) -> String {
    let verb = if engine == DatabaseEngine::MySql {
        "FOREIGN KEY"
    } else {
        "CONSTRAINT"
    };
    format!(
        "ALTER TABLE {table} DROP {verb} {};",
        engine.quote_ident(name)
    )
}

/// SQLite cannot alter columns, keys or foreign keys in place: such tables
/// are rebuilt (new table, copy of the shared columns, swap). Index-only
/// changes are applied directly.
fn sqlite_alter(
    s: &TableSchema,
    t: &TableSchema,
    changes: &[Change],
    alters: &mut Vec<String>,
    indexes: &mut Vec<String>,
) {
    let engine = DatabaseEngine::Sqlite;
    let structural = changes.iter().any(|c| {
        !matches!(
            c,
            Change::IndexAdded(_) | Change::IndexRemoved(_) | Change::IndexChanged(_)
        )
    });
    if !structural {
        for change in changes {
            match change {
                Change::IndexRemoved(i) => alters.push(drop_index(engine, &t.info.table, i)),
                Change::IndexAdded(i) | Change::IndexChanged(i) => {
                    if matches!(change, Change::IndexChanged(_)) {
                        alters.push(drop_index(engine, &t.info.table, i));
                    }
                    if let Some(index) = s.details.indexes.iter().find(|x| &x.name == i) {
                        indexes.push(create_index(engine, &s.info.table, index));
                    }
                }
                _ => {}
            }
        }
        return;
    }
    let name = &s.info.table.name;
    let temp = TableSchema {
        info: ferrumdata_domain::TableInfo {
            table: TableRef {
                schema: None,
                name: format!("_ferrum_new_{name}"),
            },
            ..s.info.clone()
        },
        details: s.details.clone(),
    };
    let shared: Vec<String> = s
        .details
        .columns
        .iter()
        .filter(|c| t.details.columns.iter().any(|o| o.name == c.name))
        .map(|c| engine.quote_ident(&c.name))
        .collect();
    let new_name = engine.quote_ident(&temp.info.table.name);
    alters.push(format!(
        "-- rebuilds {name}: SQLite cannot alter columns or keys in place"
    ));
    alters.push(create_table(engine, &temp, true));
    if !shared.is_empty() {
        alters.push(format!(
            "INSERT INTO {new_name} ({cols}) SELECT {cols} FROM {};",
            engine.quote_ident(name),
            cols = shared.join(", ")
        ));
    }
    alters.push(format!("DROP TABLE {};", engine.quote_ident(name)));
    alters.push(format!(
        "ALTER TABLE {new_name} RENAME TO {};",
        engine.quote_ident(name)
    ));
    for index in s.details.indexes.iter().filter(|i| !i.primary) {
        indexes.push(create_index(engine, &s.info.table, index));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{TableDetails, TableInfo};

    fn col(name: &str, ty: &str, nullable: bool) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            default: None,
            primary_key: name == "id",
        }
    }

    fn table(name: &str, columns: Vec<ColumnInfo>, indexes: Vec<IndexInfo>) -> TableSchema {
        TableSchema {
            info: TableInfo {
                table: TableRef {
                    schema: Some("public".into()),
                    name: name.into(),
                },
                kind: TableKind::Table,
                primary_key: vec!["id".into()],
                estimated_rows: None,
            },
            details: TableDetails {
                columns,
                indexes,
                foreign_keys: Vec::new(),
            },
        }
    }

    fn snapshot(tables: Vec<TableSchema>) -> SchemaSnapshot {
        SchemaSnapshot { tables, omitted: 0 }
    }

    fn index(name: &str, cols: &[&str], unique: bool) -> IndexInfo {
        IndexInfo {
            name: name.into(),
            columns: cols.iter().map(|c| (*c).to_owned()).collect(),
            unique,
            primary: false,
        }
    }

    fn pair() -> (SchemaSnapshot, SchemaSnapshot) {
        let source = snapshot(vec![
            table(
                "users",
                vec![
                    col("id", "integer", false),
                    col("email", "text", false),
                    col("age", "bigint", true),
                ],
                vec![index("users_email", &["email"], true)],
            ),
            table("audit", vec![col("id", "integer", false)], Vec::new()),
        ]);
        let target = snapshot(vec![
            table(
                "users",
                vec![
                    col("id", "integer", false),
                    col("email", "text", true),
                    col("age", "integer", true),
                    col("legacy", "text", true),
                ],
                vec![
                    index("users_email", &["email"], false),
                    index("old_idx", &["legacy"], false),
                ],
            ),
            table("gone", vec![col("id", "integer", false)], Vec::new()),
        ]);
        (source, target)
    }

    #[test]
    fn finds_table_column_and_index_differences() {
        let (source, target) = pair();
        let d = diff(DatabaseEngine::Postgres, &source, &target);
        let users = &d.tables[0];
        assert_eq!(users.table.name, "users");
        assert_eq!(
            users.changes,
            vec![
                Change::ColumnChanged("email".into(), vec!["NOT NULL".into()]),
                Change::ColumnChanged("age".into(), vec!["type integer → bigint".into()]),
                Change::ColumnRemoved("legacy".into()),
                Change::IndexChanged("users_email".into()),
                Change::IndexRemoved("old_idx".into()),
            ]
        );
        assert_eq!(d.tables[1].changes, vec![Change::TableAdded]);
        assert_eq!(d.tables[2].changes, vec![Change::TableRemoved]);
        assert_eq!(
            diff(DatabaseEngine::Postgres, &source, &source).identical,
            2
        );
        assert!(diff(DatabaseEngine::Postgres, &source, &source).is_empty());
    }

    #[test]
    fn postgres_migration_alters_in_order() {
        let (source, target) = pair();
        let d = diff(DatabaseEngine::Postgres, &source, &target);
        let script = migration(DatabaseEngine::Postgres, &d, &source, &target);
        let lines: Vec<&str> = script
            .lines()
            .filter(|l| !l.starts_with("--") && !l.is_empty())
            .collect();
        assert_eq!(
            lines,
            vec![
                r#"DROP INDEX "public"."users_email";"#,
                r#"DROP INDEX "public"."old_idx";"#,
                r#"DROP TABLE "public"."gone";"#,
                "CREATE TABLE \"public\".\"audit\" (",
                "  \"id\" integer NOT NULL,",
                "  PRIMARY KEY (\"id\")",
                ");",
                r#"ALTER TABLE "public"."users" ALTER COLUMN "email" SET NOT NULL;"#,
                r#"ALTER TABLE "public"."users" ALTER COLUMN "age" TYPE bigint USING "age"::bigint;"#,
                r#"ALTER TABLE "public"."users" DROP COLUMN "legacy";"#,
                r#"CREATE UNIQUE INDEX "users_email" ON "public"."users" (email);"#,
            ]
        );
        assert!(script.contains("-- drops table gone and its data"));
        let same = migration(
            DatabaseEngine::Postgres,
            &SchemaDiff::default(),
            &source,
            &source,
        );
        assert!(same.contains("nothing to do"));
    }

    #[test]
    fn mysql_modifies_whole_columns() {
        let (source, target) = pair();
        let d = diff(DatabaseEngine::MySql, &source, &target);
        let script = migration(DatabaseEngine::MySql, &d, &source, &target);
        assert!(
            script.contains("ALTER TABLE `users` MODIFY COLUMN `email` text NOT NULL;"),
            "{script}"
        );
        assert!(
            script.contains("DROP INDEX `old_idx` ON `users`;"),
            "{script}"
        );
    }

    #[test]
    fn sqlite_rebuilds_tables_with_column_changes() {
        let (source, target) = pair();
        let d = diff(DatabaseEngine::Sqlite, &source, &target);
        let script = migration(DatabaseEngine::Sqlite, &d, &source, &target);
        assert!(
            script.contains("CREATE TABLE \"_ferrum_new_users\""),
            "{script}"
        );
        assert!(script.contains(
            "INSERT INTO \"_ferrum_new_users\" (\"id\", \"email\", \"age\") SELECT \"id\", \"email\", \"age\" FROM \"users\";"
        ));
        assert!(script.contains("ALTER TABLE \"_ferrum_new_users\" RENAME TO \"users\";"));
    }
}

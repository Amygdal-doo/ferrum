//! DDL written from catalog details (`TableSchema`) for SQL dumps and
//! migration scripts. It covers columns (type, NULL, default), primary keys,
//! indexes and foreign keys; CHECK constraints, triggers, identity options
//! and comments are not in the catalog details and are not reproduced.

use ferrumdata_domain::{
    ColumnInfo, DatabaseEngine, ForeignKeyInfo, IndexInfo, TableRef, TableSchema,
};

/// How a table is named in generated scripts: schema-qualified on
/// PostgreSQL; MySQL schemas are databases, so names stay bare there (the
/// script runs in whatever database it is loaded into).
#[must_use]
pub fn table_name(engine: DatabaseEngine, table: &TableRef) -> String {
    match engine {
        DatabaseEngine::Postgres => table.qualified(engine),
        _ => engine.quote_ident(&table.name),
    }
}

fn idents(engine: DatabaseEngine, names: &[String]) -> String {
    names
        .iter()
        .map(|n| engine.quote_ident(n))
        .collect::<Vec<_>>()
        .join(", ")
}

/// `"name" type NOT NULL DEFAULT expr`.
#[must_use]
pub fn column_def(engine: DatabaseEngine, column: &ColumnInfo) -> String {
    let mut out = format!("{} {}", engine.quote_ident(&column.name), column.data_type);
    if !column.nullable {
        out.push_str(" NOT NULL");
    }
    if let Some(default) = &column.default {
        out.push_str(&format!(" DEFAULT {}", default_expr(engine, default)));
    }
    out
}

/// MySQL reports literal defaults unquoted (`abc`, `0`); everything that is
/// not a number, NULL or an expression is quoted back.
fn default_expr(engine: DatabaseEngine, default: &str) -> String {
    if engine != DatabaseEngine::MySql {
        return default.to_owned();
    }
    let upper = default.to_ascii_uppercase();
    // MySQL 8 lists expression defaults without their parentheses.
    if default.ends_with(')') && !default.starts_with('(') && !upper.starts_with("CURRENT_") {
        return format!("({default})");
    }
    let expression = default.parse::<f64>().is_ok()
        || upper == "NULL"
        || upper.starts_with("CURRENT_")
        || upper.starts_with("NOW(")
        || default.starts_with('(')
        || default.starts_with('\'');
    if expression {
        default.to_owned()
    } else {
        engine.string_literal(default)
    }
}

/// Sequences a PostgreSQL `serial` default needs (`nextval('t_id_seq'::regclass)`).
#[must_use]
pub fn sequences(engine: DatabaseEngine, table: &TableSchema) -> Vec<String> {
    if engine != DatabaseEngine::Postgres {
        return Vec::new();
    }
    table
        .details
        .columns
        .iter()
        .filter_map(|c| {
            let default = c.default.as_deref()?;
            let name = default.strip_prefix("nextval('")?.split('\'').next()?;
            Some(format!("CREATE SEQUENCE IF NOT EXISTS {name};"))
        })
        .collect()
}

/// `CREATE TABLE` with columns and the primary key; foreign keys inline
/// only when `inline_foreign_keys` (SQLite cannot add them later).
#[must_use]
pub fn create_table(
    engine: DatabaseEngine,
    table: &TableSchema,
    inline_foreign_keys: bool,
) -> String {
    let mut lines: Vec<String> = table
        .details
        .columns
        .iter()
        .map(|c| format!("  {}", column_def(engine, c)))
        .collect();
    let key = primary_key(table);
    if !key.is_empty() {
        lines.push(format!("  PRIMARY KEY ({})", idents(engine, &key)));
    }
    if inline_foreign_keys {
        lines.extend(
            table
                .details
                .foreign_keys
                .iter()
                .map(|fk| format!("  {}", foreign_key_clause(engine, &table.info.table, fk))),
        );
    }
    format!(
        "CREATE TABLE {} (\n{}\n);",
        table_name(engine, &table.info.table),
        lines.join(",\n")
    )
}

/// Primary key columns in key order (from the list, else the details).
#[must_use]
pub fn primary_key(table: &TableSchema) -> Vec<String> {
    if !table.info.primary_key.is_empty() {
        return table.info.primary_key.clone();
    }
    table
        .details
        .indexes
        .iter()
        .find(|i| i.primary)
        .map(|i| i.columns.clone())
        .unwrap_or_default()
}

/// `CREATE [UNIQUE] INDEX` for the non-primary indexes; indexes on
/// expressions FerrumData cannot read back are written as comments.
#[must_use]
pub fn create_indexes(engine: DatabaseEngine, table: &TableSchema) -> Vec<String> {
    table
        .details
        .indexes
        .iter()
        .filter(|i| !i.primary)
        .map(|i| create_index(engine, &table.info.table, i))
        .collect()
}

#[must_use]
pub fn create_index(engine: DatabaseEngine, table: &TableRef, index: &IndexInfo) -> String {
    if index.columns.iter().any(|c| c == "<expression>") {
        return format!(
            "-- index {} is on an expression and is not reproduced",
            index.name
        );
    }
    // PostgreSQL index columns come from pg_get_indexdef: already SQL.
    let columns = if engine == DatabaseEngine::Postgres {
        index.columns.join(", ")
    } else {
        idents(engine, &index.columns)
    };
    format!(
        "CREATE {}INDEX {} ON {} ({columns});",
        if index.unique { "UNIQUE " } else { "" },
        engine.quote_ident(&index.name),
        table_name(engine, table)
    )
}

fn foreign_key_clause(engine: DatabaseEngine, table: &TableRef, fk: &ForeignKeyInfo) -> String {
    let mut target = fk.ref_table.clone();
    // A bare reference stays bare when both tables live in the same schema.
    if engine != DatabaseEngine::Postgres && target.schema == table.schema {
        target.schema = None;
    }
    let mut out = String::new();
    if !fk.name.is_empty() {
        out.push_str(&format!("CONSTRAINT {} ", engine.quote_ident(&fk.name)));
    }
    out.push_str(&format!(
        "FOREIGN KEY ({}) REFERENCES {} ({})",
        idents(engine, &fk.columns),
        table_name(engine, &target),
        idents(engine, &fk.ref_columns)
    ));
    for (verb, action) in [("DELETE", &fk.on_delete), ("UPDATE", &fk.on_update)] {
        if !action.is_empty() && !action.eq_ignore_ascii_case("NO ACTION") {
            out.push_str(&format!(" ON {verb} {action}"));
        }
    }
    out
}

/// `ALTER TABLE … ADD [CONSTRAINT …] FOREIGN KEY …` (PostgreSQL, MySQL).
#[must_use]
pub fn add_foreign_key(engine: DatabaseEngine, table: &TableRef, fk: &ForeignKeyInfo) -> String {
    format!(
        "ALTER TABLE {} ADD {};",
        table_name(engine, table),
        foreign_key_clause(engine, table, fk)
    )
}

/// Tables ordered so that referenced tables come before the tables that
/// reference them (cycles keep their original order).
#[must_use]
pub fn dependency_order(tables: &[&TableSchema]) -> Vec<usize> {
    let mut placed = vec![false; tables.len()];
    let mut order = Vec::with_capacity(tables.len());
    while order.len() < tables.len() {
        let ready = (0..tables.len()).find(|&i| {
            !placed[i]
                && tables[i].details.foreign_keys.iter().all(|fk| {
                    fk.ref_table == tables[i].info.table
                        || !tables
                            .iter()
                            .enumerate()
                            .any(|(j, t)| !placed[j] && t.info.table == fk.ref_table)
                })
        });
        // A cycle: take the first unplaced table as it is.
        let next = ready
            .or_else(|| placed.iter().position(|p| !p))
            .unwrap_or(0);
        placed[next] = true;
        order.push(next);
    }
    order
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdata_domain::{TableDetails, TableInfo, TableKind};

    fn column(name: &str, ty: &str, nullable: bool, default: Option<&str>) -> ColumnInfo {
        ColumnInfo {
            name: name.into(),
            data_type: ty.into(),
            nullable,
            default: default.map(Into::into),
            primary_key: false,
        }
    }

    fn table(name: &str, refs: &[&str]) -> TableSchema {
        TableSchema {
            info: TableInfo {
                table: TableRef {
                    schema: Some("app".into()),
                    name: name.into(),
                },
                kind: TableKind::Table,
                primary_key: vec!["id".into()],
                estimated_rows: None,
            },
            details: TableDetails {
                columns: vec![
                    column(
                        "id",
                        "integer",
                        false,
                        Some("nextval('app.t_id_seq'::regclass)"),
                    ),
                    column("Note", "text", true, Some("'x'::text")),
                ],
                indexes: vec![IndexInfo {
                    name: "t_note".into(),
                    columns: vec!["lower(\"Note\")".into()],
                    unique: true,
                    primary: false,
                }],
                foreign_keys: refs
                    .iter()
                    .map(|r| ForeignKeyInfo {
                        name: format!("{name}_{r}_fk"),
                        columns: vec!["id".into()],
                        ref_table: TableRef {
                            schema: Some("app".into()),
                            name: (*r).into(),
                        },
                        ref_columns: vec!["id".into()],
                        on_delete: "CASCADE".into(),
                        on_update: "NO ACTION".into(),
                    })
                    .collect(),
            },
        }
    }

    #[test]
    fn postgres_tables_keep_schema_sequences_and_index_sql() {
        let pg = DatabaseEngine::Postgres;
        let t = table("t", &["p"]);
        assert_eq!(
            create_table(pg, &t, false),
            "CREATE TABLE \"app\".\"t\" (\n  \"id\" integer NOT NULL DEFAULT nextval('app.t_id_seq'::regclass),\n  \"Note\" text DEFAULT 'x'::text,\n  PRIMARY KEY (\"id\")\n);"
        );
        assert_eq!(
            sequences(pg, &t),
            vec!["CREATE SEQUENCE IF NOT EXISTS app.t_id_seq;"]
        );
        assert_eq!(
            create_indexes(pg, &t),
            vec!["CREATE UNIQUE INDEX \"t_note\" ON \"app\".\"t\" (lower(\"Note\"));"]
        );
        assert_eq!(
            add_foreign_key(pg, &t.info.table, &t.details.foreign_keys[0]),
            "ALTER TABLE \"app\".\"t\" ADD CONSTRAINT \"t_p_fk\" FOREIGN KEY (\"id\") REFERENCES \"app\".\"p\" (\"id\") ON DELETE CASCADE;"
        );
    }

    #[test]
    fn mysql_names_stay_bare_and_literal_defaults_are_quoted() {
        let my = DatabaseEngine::MySql;
        assert_eq!(
            column_def(my, &column("c", "varchar(5)", true, Some("abc"))),
            "`c` varchar(5) DEFAULT 'abc'"
        );
        assert_eq!(
            column_def(my, &column("n", "int", false, Some("0"))),
            "`n` int NOT NULL DEFAULT 0"
        );
        assert_eq!(
            column_def(
                my,
                &column("t", "timestamp", true, Some("CURRENT_TIMESTAMP"))
            ),
            "`t` timestamp DEFAULT CURRENT_TIMESTAMP"
        );
        let t = table("t", &["p"]);
        assert!(create_table(my, &t, true).starts_with("CREATE TABLE `t` ("));
        assert!(create_table(my, &t, true).contains("REFERENCES `p` (`id`) ON DELETE CASCADE"));
        assert!(sequences(my, &t).is_empty());
        assert_eq!(
            column_def(my, &column("u", "char(36)", true, Some("uuid()"))),
            "`u` char(36) DEFAULT (uuid())"
        );
    }

    #[test]
    fn referenced_tables_come_first_and_cycles_terminate() {
        let (a, b, c) = (table("a", &["b"]), table("b", &["c"]), table("c", &[]));
        assert_eq!(dependency_order(&[&a, &b, &c]), vec![2, 1, 0]);
        let (x, y) = (table("x", &["y"]), table("y", &["x"]));
        assert_eq!(dependency_order(&[&x, &y]).len(), 2);
        let selfref = table("s", &["s"]);
        assert_eq!(dependency_order(&[&selfref]), vec![0]);
    }
}

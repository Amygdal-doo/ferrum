//! Table listing from `INFO FOR DB`.

use ferrumdata_domain::{TableInfo, TableKind, TableRef};
use surrealdb::types::Value;

/// Extracts tables from an `INFO FOR DB` result: `{ tables: { name: "DEFINE TABLE …" } }`.
/// Every SurrealDB record has an `id`, used as the browse order key.
pub fn tables_from_info(info: Value) -> Vec<TableInfo> {
    let Value::Object(mut info) = info else {
        return Vec::new();
    };
    let Some(Value::Object(tables)) = info.remove("tables") else {
        return Vec::new();
    };
    let mut out: Vec<TableInfo> = tables
        .into_iter()
        .map(|(name, definition)| {
            let definition = match definition {
                Value::String(s) => s,
                _ => String::new(),
            };
            TableInfo {
                table: TableRef { schema: None, name },
                kind: if is_view(&definition) {
                    TableKind::View
                } else {
                    TableKind::Table
                },
                primary_key: vec!["id".to_owned()],
                estimated_rows: None,
            }
        })
        .collect();
    out.sort_by(|a, b| a.table.name.cmp(&b.table.name));
    out
}

/// `DEFINE TABLE x … AS SELECT …` declares a pre-computed view.
fn is_view(definition: &str) -> bool {
    definition.to_ascii_uppercase().contains(" AS SELECT ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use surrealdb::types::Object;

    #[test]
    fn reads_tables_and_views_sorted() {
        let mut tables = Object::new();
        tables.insert(
            "person",
            "DEFINE TABLE person TYPE NORMAL SCHEMALESS PERMISSIONS NONE".to_owned(),
        );
        tables.insert(
            "adults",
            "DEFINE TABLE adults TYPE NORMAL SCHEMALESS AS SELECT * FROM person WHERE age >= 18"
                .to_owned(),
        );
        let mut info = Object::new();
        info.insert("tables", Value::Object(tables));
        let out = tables_from_info(Value::Object(info));
        let names: Vec<_> = out.iter().map(|t| t.table.name.as_str()).collect();
        assert_eq!(names, ["adults", "person"]);
        assert_eq!(out[0].kind, TableKind::View);
        assert_eq!(out[1].primary_key, ["id"]);
    }

    #[test]
    fn unexpected_shapes_yield_nothing() {
        assert!(tables_from_info(Value::Null).is_empty());
        assert!(tables_from_info(Value::Object(Object::new())).is_empty());
    }
}

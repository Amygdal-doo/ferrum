//! Data tools (row edits, import, dump, schema compare) end to end against
//! real SQLite files, through the same services the UI uses.

use ferrumdata_application::edits::{EditTarget, NewValue, RowEdits};
use ferrumdata_application::{ConnectionService, PasswordChange, QueryService};
use ferrumdata_credentials::{CredentialStore, SessionStore};
use ferrumdata_domain::{
    CellValue, ConnectionProfile, DatabaseEngine, DbErrorKind, Endpoint, ExecutionId, ProfileId,
    QueryEvent, QueryLimits, QueryRequest,
};
use ferrumdata_persistence::AppStore;
use std::sync::Arc;
use tempfile::TempDir;

pub struct Fixture {
    _dir: TempDir,
    pub connections: ConnectionService,
    pub queries: QueryService,
    pub id: ProfileId,
    pub read_only: ProfileId,
}

/// A SQLite file with `setup` applied, plus a writable and a read-only profile.
pub async fn fixture(setup: &[&str]) -> Fixture {
    let dir = tempfile::tempdir().expect("tmp dir");
    let path = dir.path().join("t.db");
    std::fs::write(&path, b"").expect("db file");
    let store = AppStore::in_memory().await.expect("store");
    let vault: Arc<dyn CredentialStore> = Arc::new(SessionStore::new());
    let connections = ConnectionService::new(store, vault);
    let profile = |read_only: bool| {
        let mut p = ConnectionProfile::new(DatabaseEngine::Sqlite, "t");
        p.endpoint = Endpoint::File {
            path: path.to_string_lossy().into_owned(),
        };
        p.read_only = read_only;
        p
    };
    let id = save(&connections, profile(false)).await;
    let read_only = save(&connections, profile(true)).await;
    let queries = QueryService::new(connections.clone());
    let fixture = Fixture {
        _dir: dir,
        connections,
        queries,
        id,
        read_only,
    };
    for sql in setup {
        fixture.rows(sql).await;
    }
    fixture
}

async fn save(connections: &ConnectionService, profile: ConnectionProfile) -> ProfileId {
    connections
        .save_profile(profile, PasswordChange::Keep)
        .await
        .expect("save profile")
        .id
}

impl Fixture {
    /// Runs one statement on the writable profile and returns its rows.
    pub async fn rows(&self, sql: &str) -> Vec<Vec<CellValue>> {
        let request = QueryRequest {
            execution_id: ExecutionId::new(),
            profile_id: self.id,
            editor_id: None,
            sql: sql.into(),
            limits: QueryLimits::default(),
        };
        let mut rx = self.queries.execute(request).await.expect("execute");
        let mut rows = Vec::new();
        while let Some(envelope) = rx.recv().await {
            match envelope.event {
                QueryEvent::Rows(batch) => rows.extend(batch),
                QueryEvent::Failed(e) => panic!("{sql}: {e}"),
                _ => {}
            }
        }
        rows
    }
}

fn text(s: &str) -> CellValue {
    CellValue::Text(s.into())
}

#[tokio::test]
async fn staged_row_edits_apply_atomically_by_primary_key() {
    let f = fixture(&[
        "CREATE TABLE \"User\" (id INTEGER PRIMARY KEY AUTOINCREMENT, name TEXT NOT NULL, note TEXT DEFAULT 'n/a')",
        "INSERT INTO \"User\" VALUES (1, 'ann', NULL), (2, 'bob', 'x'), (3, 'cy', 'y')",
    ])
    .await;
    let tables = f.connections.list_tables(f.id).await.expect("tables");
    let columns: Vec<String> = ["id", "name", "note"].map(String::from).to_vec();
    let target = EditTarget::new(DatabaseEngine::Sqlite, &tables[0], &columns).expect("target");
    let original = f.rows("SELECT * FROM \"User\" ORDER BY id").await;

    let mut edits = RowEdits::default();
    edits.set(0, 2, NewValue::Text("it's \\ fine".into()));
    edits.set(1, 1, NewValue::Text("robert".into()));
    edits.set(1, 2, NewValue::Null);
    edits.toggle_delete(2);
    let new = edits.add_row(3);
    edits.set_new(new, 1, Some(NewValue::Text("dee".into())));
    let script = edits
        .statements(&target, |i| original.get(i).map(Vec::as_slice))
        .expect("script");
    let counts = f
        .connections
        .execute_script(f.id, script)
        .await
        .expect("apply");
    assert_eq!(counts, vec![1, 1, 1, 1]);
    assert_eq!(
        f.rows("SELECT * FROM \"User\" ORDER BY id").await,
        vec![
            vec![CellValue::Signed(1), text("ann"), text("it's \\ fine")],
            vec![CellValue::Signed(2), text("robert"), CellValue::Null],
            vec![CellValue::Signed(4), text("dee"), text("n/a")],
        ]
    );

    // The same edits again: row 3 is gone, so nothing is applied.
    let script = edits
        .statements(&target, |i| original.get(i).map(Vec::as_slice))
        .expect("script");
    let err = f
        .connections
        .execute_script(f.id, script)
        .await
        .expect_err("stale");
    assert_eq!(err.kind, DbErrorKind::Query);
    assert_eq!(
        f.rows("SELECT count(*) FROM \"User\"").await[0][0],
        CellValue::Signed(3)
    );
}

#[tokio::test]
async fn read_only_profiles_refuse_edit_scripts() {
    let f = fixture(&["CREATE TABLE t (id INTEGER PRIMARY KEY)"]).await;
    let script = vec![ferrumdata_db_driver_api::ScriptStatement::new(
        "INSERT INTO t VALUES (1)",
    )];
    let err = f
        .connections
        .execute_script(f.read_only, script)
        .await
        .expect_err("read-only");
    assert!(err.message.contains("read-only"), "{}", err.message);
    assert!(f.rows("SELECT * FROM t").await.is_empty());
}

#[tokio::test]
async fn schema_snapshot_feeds_the_er_layout() {
    let f = fixture(&[
        "CREATE TABLE customer (id INTEGER PRIMARY KEY, name TEXT)",
        "CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER REFERENCES customer(id))",
        "CREATE TABLE line (order_id INTEGER REFERENCES orders, n INTEGER, PRIMARY KEY (order_id, n))",
        "CREATE TABLE notes (body TEXT)",
        "CREATE VIEW big_orders AS SELECT * FROM orders",
    ])
    .await;
    let snapshot = f.connections.describe_schema(f.id).await.expect("snapshot");
    let names: Vec<&str> = snapshot
        .tables
        .iter()
        .map(|t| t.info.table.name.as_str())
        .collect();
    assert_eq!(
        names,
        vec!["big_orders", "customer", "line", "notes", "orders"]
    );
    let line = &snapshot.tables[2];
    assert_eq!(line.details.foreign_keys[0].ref_columns, vec!["id"]);

    let layout = ferrumdata_application::er::layout(&snapshot);
    assert_eq!(layout.edges.len(), 2);
    let x = |name: &str| {
        layout
            .nodes
            .iter()
            .find(|n| n.title == name)
            .map_or(f32::NAN, |n| n.x)
    };
    assert!(x("customer") < x("orders") && x("orders") < x("line"));
    let line_node = layout
        .nodes
        .iter()
        .find(|n| n.title == "line")
        .expect("line");
    assert!(line_node.rows[0].primary_key && line_node.rows[0].foreign_key);
}

#[tokio::test]
async fn explain_query_plan_marks_full_scans() {
    let f = fixture(&[
        "CREATE TABLE customer (id INTEGER PRIMARY KEY, name TEXT)",
        "CREATE TABLE orders (id INTEGER PRIMARY KEY, customer_id INTEGER, total REAL)",
    ])
    .await;
    let plan = ferrumdata_application::explain::run_explain(
        &f.queries,
        DatabaseEngine::Sqlite,
        f.id,
        "SELECT c.name, o.total FROM orders o JOIN customer c ON c.id = o.customer_id ORDER BY o.total;",
        false,
    )
    .await
    .expect("plan");
    let rows: Vec<(&str, bool)> = plan
        .rows
        .iter()
        .map(|r| (r.node.label.as_str(), r.slow))
        .collect();
    assert!(rows.contains(&("SCAN o", true)), "{rows:?}");
    assert!(
        rows.iter()
            .any(|(label, slow)| label.starts_with("SEARCH c") && !slow),
        "{rows:?}"
    );
    assert!(
        rows.iter()
            .any(|(label, slow)| label.contains("TEMP B-TREE") && *slow)
    );
    let err = ferrumdata_application::explain::run_explain(
        &f.queries,
        DatabaseEngine::Sqlite,
        f.id,
        "SELECT 1",
        true,
    )
    .await
    .expect_err("no analyze on SQLite");
    assert_eq!(err.kind, DbErrorKind::Unsupported);
}

/// The dump without its BEGIN / COMMIT: `execute_script` brings its own.
fn script_body(dump: &str) -> String {
    dump.lines()
        .filter(|l| *l != "BEGIN;" && *l != "COMMIT;")
        .collect::<Vec<_>>()
        .join("\n")
}

#[tokio::test]
async fn sql_dump_round_trips_schema_and_data() {
    let f = fixture(&[
        "CREATE TABLE parent (id INTEGER PRIMARY KEY, name TEXT NOT NULL DEFAULT 'x')",
        "CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent(id) ON DELETE CASCADE, \
         note TEXT, data BLOB, score REAL)",
        "CREATE UNIQUE INDEX child_note ON child(note)",
        "INSERT INTO parent VALUES (1, 'it''s'), (2, 'b\\c')",
        "INSERT INTO child VALUES (10, 1, NULL, X'00ff', 1.5), (11, 2, '', NULL, -2.0)",
    ])
    .await;
    let snapshot = f.connections.describe_schema(f.id).await.expect("snapshot");
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("dump.sql");
    let summary = ferrumdata_application::dump::write_dump(
        &f.queries,
        DatabaseEngine::Sqlite,
        f.id,
        &snapshot.tables,
        ferrumdata_application::dump::DumpOptions {
            schema: true,
            data: true,
        },
        &path,
    )
    .await
    .expect("dump");
    assert_eq!((summary.tables, summary.rows), (2, 4));
    let dump = std::fs::read_to_string(&path).expect("read");
    // Parents are created before the tables that reference them.
    assert!(dump.find("CREATE TABLE \"parent\"") < dump.find("CREATE TABLE \"child\""));

    let copy = fixture(&[]).await;
    copy.connections
        .execute_script(
            copy.id,
            vec![ferrumdata_application::ScriptStatement::new(script_body(
                &dump,
            ))],
        )
        .await
        .expect("load dump");
    for sql in [
        "SELECT * FROM parent ORDER BY id",
        "SELECT * FROM child ORDER BY id",
        "SELECT name, \"unique\" FROM pragma_index_list('child') ORDER BY name",
    ] {
        assert_eq!(f.rows(sql).await, copy.rows(sql).await, "{sql}");
    }
    let fk = copy
        .connections
        .describe_table(
            copy.id,
            ferrumdata_domain::TableRef {
                schema: None,
                name: "child".into(),
            },
        )
        .await
        .expect("describe");
    assert_eq!(fk.foreign_keys[0].on_delete, "CASCADE");
}

#[tokio::test]
async fn csv_import_maps_columns_and_is_all_or_nothing() {
    use ferrumdata_application::import::{ImportPlan, auto_map, parse_csv};
    let f = fixture(&[
        "CREATE TABLE person (id INTEGER PRIMARY KEY, name TEXT NOT NULL, age INTEGER, \
         team TEXT DEFAULT 'none')",
    ])
    .await;
    let table = ferrumdata_domain::TableRef {
        schema: None,
        name: "person".into(),
    };
    let columns: Vec<String> = ["id", "name", "age", "team"].map(String::from).to_vec();
    let csv = parse_csv("Name;Age;ID\n\"Doe; Jane\";41;1\nBob;;2\n", None);
    let plan = ImportPlan {
        header: true,
        empty_as_null: true,
        mapping: auto_map(&columns, &csv.records[0]),
    };
    let script = plan
        .statements(DatabaseEngine::Sqlite, &table, &columns, &csv)
        .expect("script");
    f.connections
        .execute_script(f.id, script)
        .await
        .expect("import");
    assert_eq!(
        f.rows("SELECT * FROM person ORDER BY id").await,
        vec![
            vec![
                CellValue::Signed(1),
                text("Doe; Jane"),
                CellValue::Signed(41),
                text("none")
            ],
            vec![
                CellValue::Signed(2),
                text("Bob"),
                CellValue::Null,
                text("none")
            ],
        ]
    );

    // A NOT NULL violation in the second batch leaves the table unchanged.
    let mut rows = String::from("id,name\n");
    for i in 10..150 {
        rows.push_str(&format!("{i},n{i}\n"));
    }
    rows.push_str("150,\n");
    let csv = parse_csv(&rows, None);
    let plan = ImportPlan {
        header: true,
        empty_as_null: true,
        mapping: auto_map(&columns, &csv.records[0]),
    };
    let script = plan
        .statements(DatabaseEngine::Sqlite, &table, &columns, &csv)
        .expect("script");
    assert_eq!(script.len(), 2);
    f.connections
        .execute_script(f.id, script)
        .await
        .expect_err("not null");
    assert_eq!(
        f.rows("SELECT count(*) FROM person").await[0][0],
        CellValue::Signed(2)
    );
}

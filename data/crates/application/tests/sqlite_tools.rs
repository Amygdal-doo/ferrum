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

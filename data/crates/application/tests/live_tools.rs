//! Data tools (EXPLAIN, SQL dump, schema compare) against real PostgreSQL and
//! MySQL servers, through the services.
//! Skipped unless `FERRUMDATA_TEST_PG=1` / `FERRUMDATA_TEST_MYSQL=1` (same
//! servers and overrides as the driver tests, see `docker-compose.yml`).

use ferrumdata_application::explain::{Plan, run_explain};
use ferrumdata_application::{ConnectionService, PasswordChange, QueryService};
use ferrumdata_credentials::{CredentialStore, SessionStore};
use ferrumdata_domain::{ConnectionProfile, DatabaseEngine, Endpoint, Secret, TlsMode};
use ferrumdata_persistence::AppStore;
use std::sync::Arc;

fn env(prefix: &str, key: &str, default: &str) -> String {
    std::env::var(format!("FERRUMDATA_TEST_{prefix}_{key}")).unwrap_or_else(|_| default.to_owned())
}

async fn services(
    engine: DatabaseEngine,
    prefix: &str,
    port: &str,
) -> Option<(
    QueryService,
    ConnectionService,
    ferrumdata_domain::ProfileId,
)> {
    if std::env::var(format!("FERRUMDATA_TEST_{prefix}")).as_deref() != Ok("1") {
        eprintln!("skipping: set FERRUMDATA_TEST_{prefix}=1 to run");
        return None;
    }
    let store = AppStore::in_memory().await.expect("store");
    let vault: Arc<dyn CredentialStore> = Arc::new(SessionStore::new());
    let connections = ConnectionService::new(store, vault);
    let mut profile = ConnectionProfile::new(engine, "it");
    profile.endpoint = Endpoint::Tcp {
        host: env(prefix, "HOST", "127.0.0.1"),
        port: env(prefix, "PORT", port).parse().expect("port"),
    };
    profile.username = Some(env(prefix, "USER", "ferrumdata"));
    profile.database = Some(env(prefix, "DB", "ferrumdata_test"));
    profile.tls = TlsMode::Disable;
    let password = Secret::new(env(prefix, "PASSWORD", "ferrumdata-dev-password"));
    let id = connections
        .save_profile(
            profile,
            PasswordChange::Set {
                secret: password,
                remember: false,
            },
        )
        .await
        .expect("save")
        .id;
    Some((QueryService::new(connections.clone()), connections, id))
}

fn labels(plan: &Plan) -> Vec<String> {
    plan.rows.iter().map(|r| r.node.label.clone()).collect()
}

#[tokio::test]
async fn postgres_plans_with_and_without_analyze() {
    let Some((queries, _, id)) = services(DatabaseEngine::Postgres, "PG", "54329").await else {
        return;
    };
    let sql = "SELECT a.x FROM generate_series(1, 5000) a(x) \
               JOIN generate_series(1, 50) b(y) ON a.x = b.y ORDER BY a.x";
    let plan = run_explain(&queries, DatabaseEngine::Postgres, id, sql, false)
        .await
        .expect("explain");
    assert!(!plan.analyzed && plan.execution_ms.is_none());
    assert!(plan.rows[0].node.cost.is_some_and(|c| c > 0.0));
    assert!(plan.rows.len() >= 3, "{:?}", labels(&plan));
    assert!(plan.rows.iter().any(|r| r.slow), "{:?}", labels(&plan));

    let plan = run_explain(&queries, DatabaseEngine::Postgres, id, sql, true)
        .await
        .expect("explain analyze");
    assert!(plan.analyzed && plan.execution_ms.is_some());
    assert_eq!(plan.rows[0].node.actual_rows, Some(50.0));
    assert!(plan.rows.iter().all(|r| r.node.total_ms.is_some()));
}

#[tokio::test]
async fn mysql_tree_plans_with_and_without_analyze() {
    let Some((queries, connections, id)) = services(DatabaseEngine::MySql, "MYSQL", "33069").await
    else {
        return;
    };
    let table = format!("x_{}", ferrumdata_domain::ExecutionId::new().0.simple());
    connections
        .execute_script(
            id,
            vec![
                ferrumdata_application::ScriptStatement::new(format!(
                    "CREATE TABLE {table} (id INT PRIMARY KEY, v INT)"
                )),
                ferrumdata_application::ScriptStatement::new(format!(
                    "INSERT INTO {table} VALUES (1, 10), (2, 20), (3, 30)"
                )),
            ],
        )
        .await
        .expect("setup");
    let sql = format!("SELECT a.v FROM {table} a JOIN {table} b ON b.id = a.id WHERE a.v > 5");
    let plan = run_explain(&queries, DatabaseEngine::MySql, id, &sql, false)
        .await
        .expect("explain");
    assert!(plan.rows.len() >= 2, "{:?}", labels(&plan));
    assert!(plan.rows[0].node.cost.is_some(), "{:?}", plan.rows[0]);
    assert!(plan.rows.iter().any(|r| r.depth == 1));

    let plan = run_explain(&queries, DatabaseEngine::MySql, id, &sql, true)
        .await
        .expect("explain analyze");
    assert_eq!(
        plan.rows[0].node.actual_rows,
        Some(3.0),
        "{:?}",
        plan.rows[0]
    );
    assert!(plan.rows[0].node.total_ms.is_some());
    connections
        .execute_script(
            id,
            vec![ferrumdata_application::ScriptStatement::new(format!(
                "DROP TABLE {table}"
            ))],
        )
        .await
        .expect("drop");
}

async fn run_rows(
    queries: &QueryService,
    id: ferrumdata_domain::ProfileId,
    sql: &str,
) -> Vec<Vec<ferrumdata_domain::CellValue>> {
    let request = ferrumdata_domain::QueryRequest {
        execution_id: ferrumdata_domain::ExecutionId::new(),
        profile_id: id,
        editor_id: None,
        sql: sql.into(),
        limits: ferrumdata_domain::QueryLimits::default(),
    };
    let mut rx = queries.execute(request).await.expect("execute");
    let mut rows = Vec::new();
    while let Some(envelope) = rx.recv().await {
        match envelope.event {
            ferrumdata_domain::QueryEvent::Rows(batch) => rows.extend(batch),
            ferrumdata_domain::QueryEvent::Failed(e) => panic!("{sql}: {e}"),
            _ => {}
        }
    }
    rows
}

async fn script(connections: &ConnectionService, id: ferrumdata_domain::ProfileId, sql: &str) {
    connections
        .execute_script(id, vec![ferrumdata_application::ScriptStatement::new(sql)])
        .await
        .unwrap_or_else(|e| panic!("{sql}: {e}"));
}

/// Dumps `tables`, drops them with `drop`, reloads the dump (without its
/// own transaction lines) and returns the dump text.
async fn dump_and_reload(
    queries: &QueryService,
    connections: &ConnectionService,
    engine: DatabaseEngine,
    id: ferrumdata_domain::ProfileId,
    tables: Vec<ferrumdata_domain::TableSchema>,
    drop: &str,
) -> String {
    let dir = tempfile::tempdir().expect("dir");
    let path = dir.path().join("dump.sql");
    ferrumdata_application::dump::write_dump(
        queries,
        engine,
        id,
        &tables,
        ferrumdata_application::dump::DumpOptions {
            schema: true,
            data: true,
        },
        &path,
    )
    .await
    .expect("dump");
    let dump = std::fs::read_to_string(&path).expect("read");
    script(connections, id, drop).await;
    let body: Vec<&str> = dump
        .lines()
        .filter(|l| !matches!(*l, "BEGIN;" | "COMMIT;" | "START TRANSACTION;"))
        .collect();
    script(connections, id, &body.join("\n")).await;
    dump
}

#[tokio::test]
async fn postgres_dump_round_trips() {
    let Some((queries, connections, id)) = services(DatabaseEngine::Postgres, "PG", "54329").await
    else {
        return;
    };
    let schema = format!("d_{}", ferrumdata_domain::ExecutionId::new().0.simple());
    script(
        &connections,
        id,
        &format!(
            "CREATE SCHEMA {schema};
             CREATE TABLE {schema}.parent (id serial PRIMARY KEY, name text NOT NULL DEFAULT 'x');
             CREATE TABLE {schema}.\"Child\" (id int PRIMARY KEY,
               parent_id int REFERENCES {schema}.parent ON DELETE CASCADE,
               note text, amount numeric(10,2), at timestamptz, flag bool, j jsonb, b bytea);
             CREATE INDEX child_lower_note ON {schema}.\"Child\" (lower(note));
             INSERT INTO {schema}.parent (name) VALUES ('it''s'), (E'back\\slash');
             INSERT INTO {schema}.\"Child\" VALUES
               (1, 1, NULL, 12.50, '2026-01-02T03:04:05+02:00', true, '{{\"a\": [1]}}', '\\x00ff'),
               (2, 2, '', -1, NULL, false, NULL, NULL);"
        ),
    )
    .await;
    let snapshot = connections.describe_schema(id).await.expect("snapshot");
    let tables: Vec<_> = snapshot
        .tables
        .into_iter()
        .filter(|t| t.info.table.schema.as_deref() == Some(schema.as_str()))
        .collect();
    assert_eq!(tables.len(), 2);
    let child = format!("SELECT * FROM {schema}.\"Child\" ORDER BY id");
    let parent = format!("SELECT * FROM {schema}.parent ORDER BY id");
    let before = (
        run_rows(&queries, id, &child).await,
        run_rows(&queries, id, &parent).await,
    );
    dump_and_reload(
        &queries,
        &connections,
        DatabaseEngine::Postgres,
        id,
        tables,
        &format!("DROP SCHEMA {schema} CASCADE"),
    )
    .await;
    let after = (
        run_rows(&queries, id, &child).await,
        run_rows(&queries, id, &parent).await,
    );
    assert_eq!(before, after);
    // The serial continues after the dumped ids, and the expression index is back.
    script(
        &connections,
        id,
        &format!("INSERT INTO {schema}.parent (name) VALUES ('z')"),
    )
    .await;
    let ids = run_rows(
        &queries,
        id,
        &format!("SELECT max(id) FROM {schema}.parent"),
    )
    .await;
    assert_eq!(ids, vec![vec![ferrumdata_domain::CellValue::Signed(3)]]);
    let indexes = run_rows(
        &queries,
        id,
        &format!("SELECT indexname::text FROM pg_indexes WHERE schemaname = '{schema}' AND indexname = 'child_lower_note'"),
    )
    .await;
    assert_eq!(indexes.len(), 1);
    script(&connections, id, &format!("DROP SCHEMA {schema} CASCADE")).await;
}

#[tokio::test]
async fn mysql_dump_round_trips() {
    let Some((queries, connections, id)) = services(DatabaseEngine::MySql, "MYSQL", "33069").await
    else {
        return;
    };
    let p = format!("p_{}", ferrumdata_domain::ExecutionId::new().0.simple());
    let c = format!("c_{}", ferrumdata_domain::ExecutionId::new().0.simple());
    script(
        &connections,
        id,
        &format!(
            "CREATE TABLE {p} (id int AUTO_INCREMENT PRIMARY KEY, name varchar(20) NOT NULL DEFAULT 'x',
               created timestamp NULL DEFAULT CURRENT_TIMESTAMP)"
        ),
    )
    .await;
    script(
        &connections,
        id,
        &format!(
            "CREATE TABLE {c} (id int PRIMARY KEY, parent_id int, note text, amount decimal(10,2),
               flag tinyint(1), b blob, UNIQUE KEY {c}_amount (amount),
               CONSTRAINT {c}_fk FOREIGN KEY (parent_id) REFERENCES {p} (id) ON DELETE CASCADE)"
        ),
    )
    .await;
    script(
        &connections,
        id,
        &format!("INSERT INTO {p} (name, created) VALUES ('it''s', '2026-01-02 03:04:05'), ('back\\\\slash', NULL)"),
    )
    .await;
    script(
        &connections,
        id,
        &format!("INSERT INTO {c} VALUES (1, 1, NULL, 12.50, 1, X'00ff'), (2, 2, '', -1, 0, NULL)"),
    )
    .await;
    let snapshot = connections.describe_schema(id).await.expect("snapshot");
    let tables: Vec<_> = snapshot
        .tables
        .into_iter()
        .filter(|t| t.info.table.name == p || t.info.table.name == c)
        .collect();
    assert_eq!(tables.len(), 2);
    let queries_sql = [
        format!("SELECT * FROM {p} ORDER BY id"),
        format!("SELECT * FROM {c} ORDER BY id"),
    ];
    let mut before = Vec::new();
    for sql in &queries_sql {
        before.push(run_rows(&queries, id, sql).await);
    }
    dump_and_reload(
        &queries,
        &connections,
        DatabaseEngine::MySql,
        id,
        tables,
        &format!("DROP TABLE {c}, {p}"),
    )
    .await;
    let mut after = Vec::new();
    for sql in &queries_sql {
        after.push(run_rows(&queries, id, sql).await);
    }
    assert_eq!(before, after);
    let details = connections
        .describe_table(
            id,
            ferrumdata_domain::TableRef {
                schema: None,
                name: c.clone(),
            },
        )
        .await
        .expect("describe");
    assert_eq!(details.foreign_keys.len(), 1);
    assert!(details.indexes.iter().any(|i| i.unique && !i.primary));
    script(&connections, id, &format!("DROP TABLE {c}, {p}")).await;
}

/// The tables of `schema`, relabelled as living in `as_schema`.
async fn schema_tables(
    connections: &ConnectionService,
    id: ferrumdata_domain::ProfileId,
    schema: &str,
    as_schema: &str,
) -> ferrumdata_domain::SchemaSnapshot {
    let mut snapshot = connections.describe_schema(id).await.expect("snapshot");
    snapshot
        .tables
        .retain(|t| t.info.table.schema.as_deref() == Some(schema));
    for t in &mut snapshot.tables {
        t.info.table.schema = Some(as_schema.to_owned());
        for fk in &mut t.details.foreign_keys {
            fk.ref_table.schema = Some(as_schema.to_owned());
        }
    }
    snapshot
}

#[tokio::test]
async fn postgres_compare_migration_makes_the_target_match() {
    use ferrumdata_application::compare::{diff, migration};
    let Some((_, connections, id)) = services(DatabaseEngine::Postgres, "PG", "54329").await else {
        return;
    };
    let suffix = ferrumdata_domain::ExecutionId::new().0.simple().to_string();
    let (src, dst) = (format!("cs_{suffix}"), format!("ct_{suffix}"));
    script(
        &connections,
        id,
        &format!(
            "CREATE SCHEMA {src};
             CREATE TABLE {src}.team (id serial PRIMARY KEY, name varchar(80) NOT NULL);
             CREATE TABLE {src}.person (id int PRIMARY KEY, name text NOT NULL DEFAULT 'n/a',
               age bigint, team_id int CONSTRAINT person_team REFERENCES {src}.team ON DELETE SET NULL);
             CREATE UNIQUE INDEX person_name ON {src}.person (lower(name));
             CREATE SCHEMA {dst};
             CREATE TABLE {dst}.person (id int PRIMARY KEY, name text, age int, legacy text);
             CREATE INDEX person_legacy ON {dst}.person (legacy);
             CREATE TABLE {dst}.old_stuff (id int PRIMARY KEY);
             INSERT INTO {dst}.person VALUES (1, 'ann', 41, 'x');"
        ),
    )
    .await;
    let engine = DatabaseEngine::Postgres;
    let source = schema_tables(&connections, id, &src, &dst).await;
    let target = schema_tables(&connections, id, &dst, &dst).await;
    let changes = diff(engine, &source, &target);
    assert_eq!(changes.tables.len(), 3, "{changes:?}");
    let sql = migration(engine, &changes, &source, &target);
    connections
        .execute_script(
            id,
            vec![ferrumdata_application::ScriptStatement::new(sql.clone())],
        )
        .await
        .unwrap_or_else(|e| panic!("{e}\n{sql}"));
    let after = schema_tables(&connections, id, &dst, &dst).await;
    let left = diff(engine, &source, &after);
    assert!(left.is_empty(), "{left:?}\n{sql}");
    script(
        &connections,
        id,
        &format!("DROP SCHEMA {src} CASCADE; DROP SCHEMA {dst} CASCADE"),
    )
    .await;
}

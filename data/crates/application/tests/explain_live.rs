//! EXPLAIN against real PostgreSQL and MySQL servers, through the services.
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

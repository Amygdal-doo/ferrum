//! Service-level tests using an in-memory store and a fake driver.

use async_trait::async_trait;
use ferrumdata_application::{ConnectionService, PasswordChange, QueryService};
use ferrumdata_credentials::{CredentialStore, SessionStore};
use ferrumdata_db_driver_api::{
    CancellationHandle, DatabaseDriver, DbConnection, DbResult, QueryExecution, ServerInfo,
    event_channel,
};
use ferrumdata_domain::{
    CapabilitySet, CellValue, ConnectionProfile, DatabaseEngine, ExecutionId, QueryEvent,
    QueryLimits, QueryOutcome, QueryRequest, ResolvedConnectionConfig, Secret, SecretRef,
};
use ferrumdata_persistence::AppStore;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Emits `batches` row batches, or blocks until cancelled when `hang` is set.
#[derive(Default)]
struct FakeDriver {
    batches: usize,
    hang: bool,
    seen_passwords: Arc<Mutex<Vec<Option<String>>>>,
}

struct FakeConn {
    batches: usize,
    hang: bool,
}

#[async_trait]
impl DatabaseDriver for FakeDriver {
    fn engine(&self) -> DatabaseEngine {
        DatabaseEngine::Postgres
    }
    fn capabilities(&self) -> CapabilitySet {
        CapabilitySet::default()
    }
    async fn connect(&self, config: &ResolvedConnectionConfig) -> DbResult<Arc<dyn DbConnection>> {
        let pw = config.password.as_ref().map(|p| p.expose().to_owned());
        self.seen_passwords.lock().expect("lock").push(pw);
        Ok(Arc::new(FakeConn {
            batches: self.batches,
            hang: self.hang,
        }))
    }
}

#[async_trait]
impl DbConnection for FakeConn {
    async fn server_info(&self) -> DbResult<ServerInfo> {
        Ok(ServerInfo {
            version: "Fake 1.0".into(),
        })
    }
    async fn list_tables(&self) -> DbResult<Vec<ferrumdata_domain::TableInfo>> {
        Ok(Vec::new())
    }
    async fn execute(&self, _request: QueryRequest) -> DbResult<QueryExecution> {
        let (tx, events) = event_channel();
        let cancellation = CancellationHandle::new();
        let cancel = cancellation.clone();
        let (batches, hang) = (self.batches, self.hang);
        tokio::spawn(async move {
            let _ = tx.send(QueryEvent::Started).await;
            if hang {
                cancel.cancelled().await;
                let _ = tx
                    .send(QueryEvent::Cancelled {
                        elapsed: Duration::ZERO,
                    })
                    .await;
                return;
            }
            for i in 0..batches {
                let _ = tx
                    .send(QueryEvent::Rows(vec![vec![CellValue::Signed(i as i64)]]))
                    .await;
            }
            let outcome = QueryOutcome {
                rows_returned: batches,
                rows_affected: None,
                truncated: false,
                elapsed: Duration::ZERO,
            };
            let _ = tx.send(QueryEvent::Completed(outcome)).await;
        });
        Ok(QueryExecution {
            events,
            cancellation,
        })
    }
    async fn close(&self) {}
}

async fn setup(driver: FakeDriver) -> (ConnectionService, Arc<SessionStore>) {
    let store = AppStore::in_memory().await.expect("store");
    let vault = Arc::new(SessionStore::new());
    let mut drivers: HashMap<DatabaseEngine, Arc<dyn DatabaseDriver>> = HashMap::new();
    drivers.insert(DatabaseEngine::Postgres, Arc::new(driver));
    let vault_dyn: Arc<dyn CredentialStore> = vault.clone();
    (
        ConnectionService::with_drivers(store, vault_dyn, drivers),
        vault,
    )
}

fn request(profile: &ConnectionProfile) -> QueryRequest {
    QueryRequest {
        execution_id: ExecutionId::new(),
        profile_id: profile.id,
        editor_id: None,
        sql: "select 1".into(),
        limits: QueryLimits::default(),
    }
}

#[tokio::test]
async fn remembered_password_goes_to_vault_not_profile() {
    let driver = FakeDriver::default();
    let seen = driver.seen_passwords.clone();
    let (svc, vault) = setup(driver).await;
    let p = ConnectionProfile::new_postgres("dev");
    let saved = svc
        .save_profile(
            p.clone(),
            PasswordChange::Set {
                secret: Secret::new("pw1"),
                remember: true,
            },
        )
        .await
        .expect("save");
    assert_eq!(saved.secret_ref, Some(SecretRef::for_profile(p.id)));
    assert_eq!(
        vault
            .get(&SecretRef::for_profile(p.id))
            .expect("get")
            .expect("some")
            .expose(),
        "pw1"
    );

    svc.connect(p.id).await.expect("connect");
    assert_eq!(
        seen.lock()
            .expect("lock")
            .last()
            .cloned()
            .flatten()
            .as_deref(),
        Some("pw1")
    );

    svc.delete_profile(p.id).await.expect("delete");
    assert!(
        vault
            .get(&SecretRef::for_profile(p.id))
            .expect("get")
            .is_none()
    );
}

#[tokio::test]
async fn session_only_password_is_not_persisted() {
    let (svc, vault) = setup(FakeDriver::default()).await;
    let p = ConnectionProfile::new_postgres("dev");
    let saved = svc
        .save_profile(
            p.clone(),
            PasswordChange::Set {
                secret: Secret::new("tmp"),
                remember: false,
            },
        )
        .await
        .expect("save");
    assert_eq!(saved.secret_ref, None);
    assert!(
        vault
            .get(&SecretRef::for_profile(p.id))
            .expect("get")
            .is_none()
    );
}

#[tokio::test]
async fn events_are_stamped_in_order_and_end_with_terminal() {
    let (svc, _) = setup(FakeDriver {
        batches: 10,
        ..FakeDriver::default()
    })
    .await;
    let p = svc
        .save_profile(ConnectionProfile::new_postgres("d"), PasswordChange::Keep)
        .await
        .expect("save");
    let queries = QueryService::new(svc);
    let req = request(&p);
    let id = req.execution_id;
    let mut rx = queries.execute(req).await.expect("execute");
    let mut seqs = Vec::new();
    let mut last = None;
    while let Some(env) = rx.recv().await {
        assert_eq!(env.execution_id, id);
        seqs.push(env.seq);
        last = Some(env.event);
    }
    assert_eq!(seqs, (1..=12).collect::<Vec<_>>());
    assert!(matches!(last, Some(QueryEvent::Completed(_))));
}

#[tokio::test]
async fn cancel_produces_cancelled_terminal_event() {
    let (svc, _) = setup(FakeDriver {
        hang: true,
        ..FakeDriver::default()
    })
    .await;
    let p = svc
        .save_profile(ConnectionProfile::new_postgres("d"), PasswordChange::Keep)
        .await
        .expect("save");
    let queries = QueryService::new(svc);
    let req = request(&p);
    let id = req.execution_id;
    let mut rx = queries.execute(req).await.expect("execute");
    assert!(matches!(
        rx.recv().await.map(|e| e.event),
        Some(QueryEvent::Started)
    ));
    assert!(queries.cancel(id));
    let terminal = tokio::time::timeout(Duration::from_secs(2), rx.recv())
        .await
        .expect("no hang");
    assert!(matches!(
        terminal.map(|e| e.event),
        Some(QueryEvent::Cancelled { .. })
    ));
    // Supervisor cleans up after the terminal event.
    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(queries.running_count(), 0);
    assert!(!queries.cancel(id));
}

#[tokio::test]
async fn default_service_registers_a_driver_for_every_engine() {
    let store = AppStore::in_memory().await.expect("store");
    let vault: Arc<dyn CredentialStore> = Arc::new(SessionStore::new());
    let svc = ConnectionService::new(store, vault);
    for engine in DatabaseEngine::ALL {
        assert!(svc.supports(engine), "{engine:?} has no driver");
    }
}

#[tokio::test]
async fn sqlite_profile_round_trips_through_the_real_driver() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("app.db");
    std::fs::write(&path, b"").expect("db file");
    let store = AppStore::in_memory().await.expect("store");
    let vault: Arc<dyn CredentialStore> = Arc::new(SessionStore::new());
    let svc = ConnectionService::new(store, vault);
    let mut p = ConnectionProfile::new(DatabaseEngine::Sqlite, "file");
    p.endpoint = ferrumdata_domain::Endpoint::File {
        path: path.to_string_lossy().into_owned(),
    };
    let saved = svc
        .save_profile(p, PasswordChange::Keep)
        .await
        .expect("save");
    let info = svc.connect(saved.id).await.expect("connect");
    assert!(info.version.starts_with("SQLite "));
    assert!(svc.list_tables(saved.id).await.expect("tables").is_empty());
    svc.disconnect(saved.id).await;
}

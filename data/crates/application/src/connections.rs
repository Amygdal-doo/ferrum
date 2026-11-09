//! Profile management, secret resolution and live connection pools.

use ferrumdata_credentials::{CredentialStore, SessionStore};
use ferrumdata_db_driver_api::{DatabaseDriver, DbConnection, ServerInfo};
use ferrumdata_db_mongodb::MongoDriver;
use ferrumdata_db_mysql::MySqlDriver;
use ferrumdata_db_postgres::PostgresDriver;
use ferrumdata_db_sqlite::SqliteDriver;
use ferrumdata_db_surrealdb::SurrealDriver;
use ferrumdata_domain::{
    ConnectionProfile, DatabaseEngine, DbError, DbErrorKind, ProfileId, ResolvedConnectionConfig,
    Secret, SecretRef,
};
use ferrumdata_persistence::AppStore;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

/// What to do with the password when saving a profile.
#[derive(Debug, Clone)]
pub enum PasswordChange {
    Keep,
    /// `remember = true` stores it in the OS vault; otherwise for this session only.
    Set {
        secret: Secret,
        remember: bool,
    },
    Clear,
}

#[derive(Clone)]
pub struct ConnectionService {
    store: AppStore,
    vault: Arc<dyn CredentialStore>,
    session: SessionStore,
    drivers: Arc<HashMap<DatabaseEngine, Arc<dyn DatabaseDriver>>>,
    live: Arc<RwLock<HashMap<ProfileId, Arc<dyn DbConnection>>>>,
}

fn internal(e: impl std::fmt::Display) -> DbError {
    DbError::new(DbErrorKind::Internal, e.to_string())
}

impl ConnectionService {
    #[must_use]
    pub fn new(store: AppStore, vault: Arc<dyn CredentialStore>) -> Self {
        let mut drivers: HashMap<DatabaseEngine, Arc<dyn DatabaseDriver>> = HashMap::new();
        drivers.insert(DatabaseEngine::Postgres, Arc::new(PostgresDriver));
        drivers.insert(DatabaseEngine::MySql, Arc::new(MySqlDriver));
        drivers.insert(DatabaseEngine::Sqlite, Arc::new(SqliteDriver));
        drivers.insert(DatabaseEngine::SurrealDb, Arc::new(SurrealDriver));
        drivers.insert(DatabaseEngine::MongoDb, Arc::new(MongoDriver));
        Self::with_drivers(store, vault, drivers)
    }

    #[must_use]
    pub fn with_drivers(
        store: AppStore,
        vault: Arc<dyn CredentialStore>,
        drivers: HashMap<DatabaseEngine, Arc<dyn DatabaseDriver>>,
    ) -> Self {
        Self {
            store,
            vault,
            session: SessionStore::new(),
            drivers: Arc::new(drivers),
            live: Arc::default(),
        }
    }

    /// Local app-state store shared with the other services.
    #[must_use]
    pub(crate) const fn store(&self) -> &AppStore {
        &self.store
    }

    #[must_use]
    pub fn vault_name(&self) -> &'static str {
        self.vault.name()
    }

    pub async fn list_profiles(&self) -> Result<Vec<ConnectionProfile>, DbError> {
        self.store.list_profiles().await.map_err(internal)
    }

    pub async fn save_profile(
        &self,
        mut profile: ConnectionProfile,
        password: PasswordChange,
    ) -> Result<ConnectionProfile, DbError> {
        let key = SecretRef::for_profile(profile.id);
        match password {
            PasswordChange::Keep => {}
            PasswordChange::Set {
                secret,
                remember: true,
            } => {
                self.vault_call(move |v| v.set(&key, &secret)).await?;
                profile.secret_ref = Some(SecretRef::for_profile(profile.id));
            }
            PasswordChange::Set {
                secret,
                remember: false,
            } => {
                self.session.set(&key, &secret).map_err(internal)?;
                let key = SecretRef::for_profile(profile.id);
                self.vault_call(move |v| v.delete(&key)).await?;
                profile.secret_ref = None;
            }
            PasswordChange::Clear => {
                self.session.delete(&key).map_err(internal)?;
                self.vault_call(move |v| v.delete(&key)).await?;
                profile.secret_ref = None;
            }
        }
        self.store
            .upsert_profile(&profile)
            .await
            .map_err(internal)?;
        // Settings changed: drop any pool built from the old profile.
        self.disconnect(profile.id).await;
        Ok(profile)
    }

    pub async fn delete_profile(&self, id: ProfileId) -> Result<(), DbError> {
        self.disconnect(id).await;
        let key = SecretRef::for_profile(id);
        self.session.delete(&key).map_err(internal)?;
        self.vault_call(move |v| v.delete(&key)).await?;
        self.store.delete_profile(id).await.map_err(internal)
    }

    /// Tries a connection without keeping it. `password` overrides stored secrets.
    pub async fn test(
        &self,
        profile: ConnectionProfile,
        password: Option<Secret>,
    ) -> Result<ServerInfo, DbError> {
        let password = match password {
            Some(p) => Some(p),
            None => self.resolve_password(&profile).await?,
        };
        let conn = self
            .driver(profile.engine)?
            .connect(&ResolvedConnectionConfig { profile, password })
            .await?;
        let info = conn.server_info().await;
        conn.close().await;
        info
    }

    /// Returns the live connection for a profile, connecting if needed.
    pub async fn ensure_connected(&self, id: ProfileId) -> Result<Arc<dyn DbConnection>, DbError> {
        if let Some(conn) = self.live.read().await.get(&id) {
            return Ok(conn.clone());
        }
        let profile = self
            .list_profiles()
            .await?
            .into_iter()
            .find(|p| p.id == id)
            .ok_or_else(|| DbError::new(DbErrorKind::Internal, "Connection profile not found"))?;
        let password = self.resolve_password(&profile).await?;
        let conn = self
            .driver(profile.engine)?
            .connect(&ResolvedConnectionConfig { profile, password })
            .await?;
        let mut live = self.live.write().await;
        // Another task may have connected concurrently; keep the first.
        if let Some(existing) = live.get(&id) {
            let existing = existing.clone();
            drop(live);
            conn.close().await;
            return Ok(existing);
        }
        live.insert(id, conn.clone());
        Ok(conn)
    }

    pub async fn connect(&self, id: ProfileId) -> Result<ServerInfo, DbError> {
        self.ensure_connected(id).await?.server_info().await
    }

    /// Tables/views for a profile, connecting if needed.
    pub async fn list_tables(
        &self,
        id: ProfileId,
    ) -> Result<Vec<ferrumdata_domain::TableInfo>, DbError> {
        self.ensure_connected(id).await?.list_tables().await
    }

    /// Columns and indexes of one table, connecting if needed.
    pub async fn describe_table(
        &self,
        id: ProfileId,
        table: ferrumdata_domain::TableRef,
    ) -> Result<ferrumdata_domain::TableDetails, DbError> {
        self.ensure_connected(id)
            .await?
            .describe_table(&table)
            .await
    }

    pub async fn disconnect(&self, id: ProfileId) {
        let removed = self.live.write().await.remove(&id);
        if let Some(conn) = removed {
            conn.close().await;
        }
    }

    pub async fn is_connected(&self, id: ProfileId) -> bool {
        self.live.read().await.contains_key(&id)
    }

    #[must_use]
    pub fn supports(&self, engine: DatabaseEngine) -> bool {
        self.drivers.contains_key(&engine)
    }

    fn driver(&self, engine: DatabaseEngine) -> Result<Arc<dyn DatabaseDriver>, DbError> {
        self.drivers
            .get(&engine)
            .cloned()
            .ok_or_else(|| DbError::unsupported(format!("{} is not supported yet", engine.label())))
    }

    /// Session password first, then the OS vault. `None` means no password.
    async fn resolve_password(
        &self,
        profile: &ConnectionProfile,
    ) -> Result<Option<Secret>, DbError> {
        let key = SecretRef::for_profile(profile.id);
        if let Some(s) = self.session.get(&key).map_err(internal)? {
            return Ok(Some(s));
        }
        match profile.secret_ref.clone() {
            Some(key) => self.vault_call(move |v| v.get(&key)).await,
            None => Ok(None),
        }
    }

    /// OS vault calls can block (e.g. a Keychain prompt): run off the async workers.
    async fn vault_call<T, F>(&self, f: F) -> Result<T, DbError>
    where
        T: Send + 'static,
        F: FnOnce(&dyn CredentialStore) -> ferrumdata_credentials::CredentialResult<T>
            + Send
            + 'static,
    {
        let vault = self.vault.clone();
        tokio::task::spawn_blocking(move || f(vault.as_ref()))
            .await
            .map_err(internal)?
            .map_err(internal)
    }
}

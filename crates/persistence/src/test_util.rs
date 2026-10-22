//! Shared test helpers.

use crate::Store;
use std::path::PathBuf;

/// A unique temp DB path; the file (and WAL side files) are removed on drop.
pub struct TempDb {
    pub path: PathBuf,
}

impl TempDb {
    pub fn new() -> Self {
        let path = std::env::temp_dir().join(format!("ferrumapi-test-{}.db", uuid::Uuid::new_v4()));
        Self { path }
    }
}

impl Drop for TempDb {
    fn drop(&mut self) {
        for suffix in ["", "-wal", "-shm"] {
            let mut p = self.path.clone().into_os_string();
            p.push(suffix);
            let _ = std::fs::remove_file(p);
        }
    }
}

pub async fn mem() -> Store {
    Store::in_memory().await.expect("open in-memory store")
}

/// Executes raw SQL against the store (for injecting corrupt rows).
pub async fn exec(store: &Store, sql: &'static str) {
    sqlx::query(sql)
        .execute(&store.pool)
        .await
        .expect("raw sql");
}

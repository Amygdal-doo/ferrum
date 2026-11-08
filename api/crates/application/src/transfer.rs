//! Import / export to files. File dialogs are the UI's job; these take paths.

use crate::AppServices;
use ferrumapi_domain::{Collection, Environment, HttpRequest};
use ferrumapi_interop::{InteropError, curl, postman};
use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum TransferError {
    #[error("file error: {0}")]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    Interop(#[from] InteropError),
    #[error(transparent)]
    Store(#[from] crate::StoreError),
}

impl AppServices {
    /// Reads a Postman v2.1 collection file, saves it and returns it.
    pub async fn import_postman_file(&self, path: PathBuf) -> Result<Collection, TransferError> {
        let json = tokio::fs::read_to_string(&path).await?;
        let collection = postman::import_collection(&json)?;
        self.store.save_collection(&collection).await?;
        Ok(collection)
    }

    /// Reads a Postman environment file, saves it and returns it.
    pub async fn import_environment_file(
        &self,
        path: PathBuf,
    ) -> Result<Environment, TransferError> {
        let json = tokio::fs::read_to_string(&path).await?;
        let env = postman::import_environment(&json)?;
        self.store.save_environment(&env).await?;
        Ok(env)
    }

    /// Writes `collection` as Postman v2.1 JSON.
    pub async fn export_postman_file(
        &self,
        collection: Collection,
        path: PathBuf,
    ) -> Result<(), TransferError> {
        let json = postman::export_collection(&collection)?;
        tokio::fs::write(&path, json).await?;
        Ok(())
    }

    /// Writes raw bytes (e.g. a response body) to `path`.
    pub async fn save_bytes(&self, bytes: Vec<u8>, path: PathBuf) -> Result<(), TransferError> {
        tokio::fs::write(&path, bytes).await?;
        Ok(())
    }

    /// Parses a cURL command into a request (no I/O).
    pub fn import_curl(&self, command: &str) -> Result<HttpRequest, TransferError> {
        Ok(curl::parse(command)?)
    }
}

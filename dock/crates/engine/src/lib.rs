//! FerrumDock engine access: a Docker Engine API client behind the [`Engine`]
//! trait (bollard over a Unix socket or TCP), engine-neutral models, socket
//! detection, formatting, log/ANSI handling, compose grouping and the
//! `docker compose` / "open shell" launchers. No UI code lives here.

pub mod ansi;
pub mod cli;
pub mod compose;
mod convert;
pub mod docker;
pub mod endpoint;
pub mod engine;
pub mod error;
pub mod fake;
pub mod format;
pub mod logs;
pub mod model;
pub mod ops;
pub mod pull;
mod rawhttp;
pub mod shell;
pub mod stats;

pub use cli::DockerCli;
pub use docker::DockerEngine;
pub use endpoint::{Candidate, Endpoint};
pub use engine::{Connection, Engine, EngineStream, SharedEngine, connect_first, sample_stats};
pub use error::{EngineError, EngineResult};
pub use model::*;

/// Detects the present candidates and connects to the first engine that answers.
pub async fn connect() -> EngineResult<Connection> {
    let candidates = endpoint::detect();
    connect_first(&candidates, |candidate| async move {
        let engine = DockerEngine::connect(&candidate.endpoint).await?;
        Ok(std::sync::Arc::new(engine) as SharedEngine)
    })
    .await
}

//! FerrumContainers engine access: a Docker Engine API client behind the [`Engine`]
//! trait (bollard over a Unix socket or TCP), engine-neutral models, socket
//! detection, formatting, log/ANSI handling, compose grouping and the
//! `docker compose` / "open shell" launchers. No UI code lives here.

pub mod ansi;
pub mod cli;
pub mod compose;
pub mod compose_ps;
pub mod context;
mod convert;
pub mod create;
pub mod docker;
pub mod endpoint;
pub mod engine;
pub mod error;
pub mod fake;
pub mod format;
pub mod logmux;
pub mod logs;
pub mod model;
pub mod ops;
pub mod project;
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

/// Detects the present candidates (Docker contexts included) and connects to
/// the first engine that answers, trying `preferred` (picked in the UI) first.
pub async fn connect(preferred: Option<Endpoint>) -> EngineResult<Connection> {
    let candidates = endpoint::prefer(endpoint::detect(), preferred.as_ref());
    connect_first(&candidates, |candidate| async move {
        let engine = DockerEngine::connect(&candidate.endpoint).await?;
        Ok(std::sync::Arc::new(engine) as SharedEngine)
    })
    .await
}

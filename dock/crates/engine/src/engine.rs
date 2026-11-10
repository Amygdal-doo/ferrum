//! The engine seam: everything the app does to the engine goes through
//! [`Engine`], implemented by [`crate::docker::DockerEngine`] (bollard) and
//! by [`crate::fake::FakeEngine`] in tests.

use crate::endpoint::Candidate;
use crate::error::{EngineError, EngineResult};
use crate::model::{
    Container, ContainerAction, ContainerDetail, EngineInfo, Image, LogChunk, LogOptions, Network,
    PruneReport, PullEvent, Stats, Volume,
};
use async_trait::async_trait;
use futures::stream::BoxStream;
use std::collections::HashMap;
use std::future::Future;
use std::sync::Arc;

pub type EngineStream<T> = BoxStream<'static, EngineResult<T>>;

#[async_trait]
pub trait Engine: Send + Sync + 'static {
    async fn info(&self) -> EngineResult<EngineInfo>;

    /// All containers, stopped ones included.
    async fn containers(&self) -> EngineResult<Vec<Container>>;
    /// One CPU/memory sample (the engine takes about a second to compute CPU).
    async fn stats(&self, id: &str) -> EngineResult<Stats>;
    async fn inspect(&self, id: &str) -> EngineResult<ContainerDetail>;
    async fn container_action(&self, id: &str, action: ContainerAction) -> EngineResult<()>;
    fn logs(&self, id: &str, options: LogOptions) -> EngineStream<LogChunk>;

    async fn images(&self) -> EngineResult<Vec<Image>>;
    /// Pulls `reference` (a missing tag means `latest`).
    fn pull(&self, reference: &str) -> EngineStream<PullEvent>;
    async fn remove_image(&self, id: &str) -> EngineResult<()>;
    /// Removes dangling (untagged) images.
    async fn prune_images(&self) -> EngineResult<PruneReport>;

    async fn volumes(&self) -> EngineResult<Vec<Volume>>;
    async fn remove_volume(&self, name: &str) -> EngineResult<()>;
    /// Removes every volume not used by a container (named ones too).
    async fn prune_volumes(&self) -> EngineResult<PruneReport>;

    async fn networks(&self) -> EngineResult<Vec<Network>>;
    async fn remove_network(&self, id: &str) -> EngineResult<()>;
    async fn prune_networks(&self) -> EngineResult<PruneReport>;
}

pub type SharedEngine = Arc<dyn Engine>;

/// A live connection: which candidate answered and what it reported.
#[derive(Clone)]
pub struct Connection {
    pub candidate: Candidate,
    pub engine: SharedEngine,
    pub info: EngineInfo,
}

/// Tries `candidates` in order and keeps the first one whose engine answers
/// `/version` (a socket file can outlive a stopped engine). The last error is
/// returned when none answers; [`EngineError::NoEngine`] when there are none.
pub async fn connect_first<F, Fut>(candidates: &[Candidate], connect: F) -> EngineResult<Connection>
where
    F: Fn(Candidate) -> Fut,
    Fut: Future<Output = EngineResult<SharedEngine>>,
{
    let mut last_error = EngineError::NoEngine;
    for candidate in candidates {
        let attempt = async {
            let engine = connect(candidate.clone()).await?;
            let info = engine.info().await?;
            Ok::<_, EngineError>((engine, info))
        };
        match attempt.await {
            Ok((engine, info)) => {
                return Ok(Connection {
                    candidate: candidate.clone(),
                    engine,
                    info,
                });
            }
            Err(error) => {
                tracing::debug!(endpoint = %candidate.endpoint, error = %error, "engine candidate did not answer");
                last_error = match error {
                    EngineError::Unreachable { .. } => error,
                    other => EngineError::Unreachable {
                        endpoint: candidate.endpoint.to_string(),
                        message: other.to_string(),
                    },
                };
            }
        }
    }
    Err(last_error)
}

/// Samples stats for `ids` concurrently; containers that fail (just stopped,
/// removed) are left out instead of failing the batch.
pub async fn sample_stats(engine: &SharedEngine, ids: &[String]) -> HashMap<String, Stats> {
    let samples = futures::future::join_all(ids.iter().map(|id| {
        let engine = engine.clone();
        async move { (id.clone(), engine.stats(id).await) }
    }))
    .await;
    samples
        .into_iter()
        .filter_map(|(id, result)| result.ok().map(|stats| (id, stats)))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::endpoint::Endpoint;
    use crate::fake::FakeEngine;

    fn candidate(path: &str, source: &'static str) -> Candidate {
        Candidate {
            endpoint: Endpoint::Unix(path.into()),
            source,
        }
    }

    #[tokio::test]
    async fn connects_to_the_first_candidate_that_answers() {
        let candidates = [
            candidate("/stale.sock", "Docker Desktop"),
            candidate("/live.sock", "OrbStack"),
        ];
        let connection = connect_first(&candidates, |c| async move {
            let engine = FakeEngine::default();
            if c.source == "Docker Desktop" {
                engine.set_down(true);
            }
            Ok(Arc::new(engine) as SharedEngine)
        })
        .await;
        let connection = connection.unwrap_or_else(|e| panic!("should connect: {e}"));
        assert_eq!(connection.candidate.source, "OrbStack");
        assert_eq!(connection.info.version, "28.0.0");
    }

    #[tokio::test]
    async fn reports_no_engine_or_the_last_failure() {
        let none = connect_first(&[], |_| async {
            Ok(Arc::new(FakeEngine::default()) as SharedEngine)
        })
        .await;
        assert!(matches!(none, Err(EngineError::NoEngine)));

        let down = connect_first(&[candidate("/x.sock", "Colima")], |_| async {
            Err(EngineError::Transport("connection refused".into()))
        })
        .await;
        match down {
            Err(EngineError::Unreachable { endpoint, message }) => {
                assert_eq!(endpoint, "/x.sock");
                assert_eq!(message, "connection refused");
            }
            other => panic!("unexpected: {:?}", other.map(|c| c.candidate)),
        }
    }

    #[tokio::test]
    async fn samples_stats_and_skips_failures() {
        let fake = FakeEngine::with_sample_data();
        let engine: SharedEngine = Arc::new(fake);
        let ids = vec!["aaa111".to_owned(), "missing".to_owned()];
        let stats = sample_stats(&engine, &ids).await;
        assert_eq!(stats.len(), 1);
        assert!(stats.contains_key("aaa111"));
    }
}

//! In-memory [`Engine`] for tests: behaves like a small Docker engine
//! (state transitions, 404/409 errors, prune semantics) and records calls.

use crate::engine::{Engine, EngineStream};
use crate::error::{EngineError, EngineResult};
use crate::model::{
    COMPOSE_PROJECT, COMPOSE_SERVICE, Container, ContainerAction, ContainerDetail, ContainerState,
    EngineInfo, Image, Labels, LogChunk, LogOptions, Network, PruneReport, PullEvent, Stats,
    Volume,
};
use async_trait::async_trait;
use futures::StreamExt;
use std::sync::{Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
pub struct FakeState {
    pub down: bool,
    pub containers: Vec<Container>,
    pub images: Vec<Image>,
    pub volumes: Vec<Volume>,
    pub networks: Vec<Network>,
    pub logs: Vec<LogChunk>,
    pub pull_events: Vec<PullEvent>,
    /// Every mutating call, e.g. `remove container web force=true`.
    pub calls: Vec<String>,
}

#[derive(Debug, Default)]
pub struct FakeEngine {
    state: Mutex<FakeState>,
}

/// A container fixture; `compose` = (project, service).
#[must_use]
pub fn container(id: &str, name: &str, compose: Option<(&str, &str)>, state: &str) -> Container {
    let mut labels = Labels::new();
    if let Some((project, service)) = compose {
        labels.insert(COMPOSE_PROJECT.into(), project.into());
        labels.insert(COMPOSE_SERVICE.into(), service.into());
    }
    Container {
        id: id.to_owned(),
        name: name.to_owned(),
        image: format!("{name}:latest"),
        state: ContainerState::parse(state),
        status: state.to_owned(),
        ports: Vec::new(),
        labels,
        created: 0,
    }
}

fn not_found(what: &str, id: &str) -> EngineError {
    EngineError::Api {
        status: 404,
        message: format!("No such {what}: {id}"),
    }
}

fn conflict(message: String) -> EngineError {
    EngineError::Api {
        status: 409,
        message,
    }
}

impl FakeEngine {
    pub fn state(&self) -> MutexGuard<'_, FakeState> {
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    /// Makes every call fail like a stopped engine.
    pub fn set_down(&self, down: bool) {
        self.state().down = down;
    }

    /// Two compose services, one standalone container, images, volumes, networks.
    #[must_use]
    pub fn with_sample_data() -> Self {
        let engine = Self::default();
        {
            let mut s = engine.state();
            s.containers = vec![
                container("aaa111", "web-app-1", Some(("web", "app")), "running"),
                container("bbb222", "web-db-1", Some(("web", "db")), "exited"),
                container("ccc333", "scratch", None, "paused"),
            ];
            let image = |id: &str, tags: &[&str], size: u64, containers: i64| Image {
                id: id.to_owned(),
                tags: tags.iter().map(|t| (*t).to_owned()).collect(),
                size,
                created: 0,
                containers,
            };
            s.images = vec![
                image("sha256:img1", &["web-app:latest"], 100, 1),
                image("sha256:img2", &[], 40, 0),
                image("sha256:img3", &[], 60, 0),
            ];
            let volume = |name: &str, refs: i64, size: u64| Volume {
                name: name.to_owned(),
                driver: "local".into(),
                mountpoint: format!("/var/lib/docker/volumes/{name}/_data"),
                created: None,
                labels: Labels::new(),
                size: Some(size),
                ref_count: Some(refs),
            };
            s.volumes = vec![volume("web_db", 1, 500), volume("old", 0, 300)];
            let network = |name: &str| Network {
                id: format!("net-{name}"),
                name: name.to_owned(),
                driver: "bridge".into(),
                scope: "local".into(),
                internal: false,
                created: None,
                labels: Labels::new(),
            };
            s.networks = vec![network("bridge"), network("host"), network("web_default")];
        }
        engine
    }

    fn guard(&self) -> EngineResult<MutexGuard<'_, FakeState>> {
        let state = self.state();
        if state.down {
            return Err(EngineError::Transport("connection refused".into()));
        }
        Ok(state)
    }
}

#[async_trait]
impl Engine for FakeEngine {
    async fn info(&self) -> EngineResult<EngineInfo> {
        drop(self.guard()?);
        Ok(EngineInfo {
            version: "28.0.0".into(),
            api_version: "1.48".into(),
            os: "linux".into(),
            arch: "arm64".into(),
            platform: Some("Fake Engine".into()),
        })
    }

    async fn containers(&self) -> EngineResult<Vec<Container>> {
        Ok(self.guard()?.containers.clone())
    }

    async fn stats(&self, id: &str) -> EngineResult<Stats> {
        let s = self.guard()?;
        let c = s
            .containers
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| not_found("container", id))?;
        Ok(if c.state.is_up() {
            Stats {
                cpu_percent: 1.5,
                mem_usage: 64 << 20,
                mem_limit: 1 << 30,
            }
        } else {
            Stats::default()
        })
    }

    async fn inspect(&self, id: &str) -> EngineResult<ContainerDetail> {
        let s = self.guard()?;
        let c = s
            .containers
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| not_found("container", id))?;
        Ok(ContainerDetail {
            id: c.id.clone(),
            name: c.name.clone(),
            image: c.image.clone(),
            state: c.state,
            created: String::new(),
            started_at: None,
            command: "sleep infinity".into(),
            working_dir: None,
            tty: false,
            restart_count: 0,
            env: vec![("PATH".into(), "/usr/bin".into())],
            mounts: Vec::new(),
            networks: Vec::new(),
            labels: c.labels.clone(),
        })
    }

    async fn container_action(&self, id: &str, action: ContainerAction) -> EngineResult<()> {
        let mut s = self.guard()?;
        s.calls.push(match action {
            ContainerAction::Remove { force } => format!("remove container {id} force={force}"),
            other => format!("{} {id}", other.verb()),
        });
        let index = s
            .containers
            .iter()
            .position(|c| c.id == id)
            .ok_or_else(|| not_found("container", id))?;
        let state = s.containers[index].state;
        let next = match action {
            ContainerAction::Start | ContainerAction::Restart => ContainerState::Running,
            ContainerAction::Stop => ContainerState::Exited,
            ContainerAction::Pause if state == ContainerState::Running => ContainerState::Paused,
            ContainerAction::Unpause if state == ContainerState::Paused => ContainerState::Running,
            ContainerAction::Pause | ContainerAction::Unpause => {
                return Err(conflict(format!(
                    "Container {id} is not {}",
                    if action == ContainerAction::Pause {
                        "running"
                    } else {
                        "paused"
                    }
                )));
            }
            ContainerAction::Remove { force } => {
                if state.can_stop() && !force {
                    return Err(conflict(format!(
                        "Cannot remove container {id}: container is {}",
                        state.label()
                    )));
                }
                s.containers.remove(index);
                return Ok(());
            }
        };
        s.containers[index].state = next;
        Ok(())
    }

    fn logs(&self, _id: &str, _options: LogOptions) -> EngineStream<LogChunk> {
        let chunks = self.state().logs.clone();
        futures::stream::iter(chunks.into_iter().map(Ok)).boxed()
    }

    async fn images(&self) -> EngineResult<Vec<Image>> {
        Ok(self.guard()?.images.clone())
    }

    fn pull(&self, reference: &str) -> EngineStream<PullEvent> {
        let mut s = self.state();
        s.calls.push(format!("pull {reference}"));
        let events = s.pull_events.clone();
        futures::stream::iter(events.into_iter().map(Ok)).boxed()
    }

    async fn remove_image(&self, id: &str) -> EngineResult<()> {
        let mut s = self.guard()?;
        s.calls.push(format!("remove image {id}"));
        let index = s
            .images
            .iter()
            .position(|i| i.id == id)
            .ok_or_else(|| not_found("image", id))?;
        if s.images[index].containers > 0 {
            return Err(conflict(format!(
                "Unable to remove image {id}: image is being used"
            )));
        }
        s.images.remove(index);
        Ok(())
    }

    async fn prune_images(&self) -> EngineResult<PruneReport> {
        let mut s = self.guard()?;
        s.calls.push("prune images".into());
        let (gone, kept): (Vec<Image>, Vec<Image>) = s
            .images
            .drain(..)
            .partition(|i| i.is_dangling() && i.containers <= 0);
        s.images = kept;
        Ok(PruneReport {
            reclaimed: gone.iter().map(|i| i.size).sum(),
            deleted: gone.into_iter().map(|i| i.id).collect(),
        })
    }

    async fn volumes(&self) -> EngineResult<Vec<Volume>> {
        Ok(self.guard()?.volumes.clone())
    }

    async fn remove_volume(&self, name: &str) -> EngineResult<()> {
        let mut s = self.guard()?;
        s.calls.push(format!("remove volume {name}"));
        let index = s
            .volumes
            .iter()
            .position(|v| v.name == name)
            .ok_or_else(|| not_found("volume", name))?;
        if s.volumes[index].ref_count.unwrap_or(0) > 0 {
            return Err(conflict(format!("Remove {name}: volume is in use")));
        }
        s.volumes.remove(index);
        Ok(())
    }

    async fn prune_volumes(&self) -> EngineResult<PruneReport> {
        let mut s = self.guard()?;
        s.calls.push("prune volumes".into());
        let (gone, kept): (Vec<Volume>, Vec<Volume>) = s
            .volumes
            .drain(..)
            .partition(|v| v.ref_count.unwrap_or(0) == 0);
        s.volumes = kept;
        Ok(PruneReport {
            reclaimed: gone.iter().filter_map(|v| v.size).sum(),
            deleted: gone.into_iter().map(|v| v.name).collect(),
        })
    }

    async fn networks(&self) -> EngineResult<Vec<Network>> {
        Ok(self.guard()?.networks.clone())
    }

    async fn remove_network(&self, id: &str) -> EngineResult<()> {
        let mut s = self.guard()?;
        s.calls.push(format!("remove network {id}"));
        let index = s
            .networks
            .iter()
            .position(|n| n.id == id)
            .ok_or_else(|| not_found("network", id))?;
        if s.networks[index].is_builtin() {
            return Err(EngineError::Api {
                status: 403,
                message: format!(
                    "{} is a pre-defined network and cannot be removed",
                    s.networks[index].name
                ),
            });
        }
        s.networks.remove(index);
        Ok(())
    }

    async fn prune_networks(&self) -> EngineResult<PruneReport> {
        let mut s = self.guard()?;
        s.calls.push("prune networks".into());
        let (gone, kept): (Vec<Network>, Vec<Network>) =
            s.networks.drain(..).partition(|n| !n.is_builtin());
        s.networks = kept;
        Ok(PruneReport {
            deleted: gone.into_iter().map(|n| n.name).collect(),
            reclaimed: 0,
        })
    }
}

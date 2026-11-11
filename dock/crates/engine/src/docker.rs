//! [`Engine`] over the Docker Engine API with bollard.

use crate::convert;
use crate::create::{CreateSpec, create_body};
use crate::endpoint::Endpoint;
use crate::engine::{Engine, EngineStream};
use crate::error::{EngineError, EngineResult};
use crate::model::{
    Container, ContainerAction, ContainerDetail, EngineInfo, Image, ImageLayer, LogChunk,
    LogOptions, LogStream, Network, PruneReport, PullEvent, Stats, Volume,
};
use crate::rawhttp;
use async_trait::async_trait;
use bollard::Docker;
use bollard::container::LogOutput;
use bollard::query_parameters as q;
use futures::{StreamExt, TryStreamExt};
use std::collections::HashMap;

/// Seconds before a request without a response fails (streams are not cut).
const TIMEOUT_SECS: u64 = 30;
const DF_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);

#[derive(Clone)]
pub struct DockerEngine {
    docker: Docker,
    endpoint: Endpoint,
}

impl DockerEngine {
    /// Connects to `endpoint` and negotiates the API version with the engine.
    pub async fn connect(endpoint: &Endpoint) -> EngineResult<Self> {
        let unreachable = |e: bollard::errors::Error| EngineError::Unreachable {
            endpoint: endpoint.to_string(),
            message: e.to_string(),
        };
        let docker = match endpoint {
            Endpoint::Unix(path) => Docker::connect_with_unix(
                &path.to_string_lossy(),
                TIMEOUT_SECS,
                bollard::API_DEFAULT_VERSION,
            ),
            Endpoint::Tcp(addr) => {
                Docker::connect_with_http(addr, TIMEOUT_SECS, bollard::API_DEFAULT_VERSION)
            }
        }
        .map_err(unreachable)?;
        let docker = docker.negotiate_version().await.map_err(unreachable)?;
        Ok(Self {
            docker,
            endpoint: endpoint.clone(),
        })
    }
}

fn label_filter(key: &str, value: &str) -> Option<HashMap<String, Vec<String>>> {
    Some(HashMap::from([(key.to_owned(), vec![value.to_owned()])]))
}

#[async_trait]
impl Engine for DockerEngine {
    async fn info(&self) -> EngineResult<EngineInfo> {
        Ok(convert::engine_info(self.docker.version().await?))
    }

    async fn containers(&self) -> EngineResult<Vec<Container>> {
        let options = q::ListContainersOptionsBuilder::new().all(true).build();
        let list = self.docker.list_containers(Some(options)).await?;
        Ok(list.into_iter().map(convert::container).collect())
    }

    async fn stats(&self, id: &str) -> EngineResult<Stats> {
        // stream=false without one_shot: the engine waits for a second sample
        // so `precpu_stats` is filled and CPU % is meaningful.
        let options = q::StatsOptions {
            stream: false,
            one_shot: false,
        };
        let mut stream = self.docker.stats(id, Some(options));
        match stream.next().await {
            Some(sample) => Ok(convert::stats(&sample?)),
            None => Err(EngineError::Transport("no stats returned".into())),
        }
    }

    async fn inspect(&self, id: &str) -> EngineResult<ContainerDetail> {
        let detail = self
            .docker
            .inspect_container(id, None::<q::InspectContainerOptions>)
            .await?;
        Ok(convert::detail(detail))
    }

    async fn container_action(&self, id: &str, action: ContainerAction) -> EngineResult<()> {
        let d = &self.docker;
        match action {
            ContainerAction::Start => {
                d.start_container(id, None::<q::StartContainerOptions>)
                    .await?
            }
            ContainerAction::Stop => {
                d.stop_container(id, None::<q::StopContainerOptions>)
                    .await?
            }
            ContainerAction::Restart => {
                d.restart_container(id, None::<q::RestartContainerOptions>)
                    .await?;
            }
            ContainerAction::Pause => d.pause_container(id).await?,
            ContainerAction::Unpause => d.unpause_container(id).await?,
            ContainerAction::Remove { force } => {
                let options = q::RemoveContainerOptionsBuilder::new().force(force).build();
                d.remove_container(id, Some(options)).await?;
            }
        }
        Ok(())
    }

    fn logs(&self, id: &str, options: LogOptions) -> EngineStream<LogChunk> {
        let tail = options
            .tail
            .map_or_else(|| "all".to_owned(), |n| n.to_string());
        let since = options
            .since
            .and_then(|s| i32::try_from(s).ok())
            .unwrap_or(0);
        let options = q::LogsOptionsBuilder::new()
            .follow(options.follow)
            .stdout(true)
            .stderr(true)
            .timestamps(options.timestamps)
            .since(since)
            .tail(&tail)
            .build();
        self.docker
            .logs(id, Some(options))
            .map_ok(|output| match output {
                LogOutput::StdErr { message } => LogChunk {
                    stream: LogStream::Stderr,
                    text: String::from_utf8_lossy(&message).into_owned(),
                },
                LogOutput::StdOut { message }
                | LogOutput::Console { message }
                | LogOutput::StdIn { message } => LogChunk {
                    stream: LogStream::Stdout,
                    text: String::from_utf8_lossy(&message).into_owned(),
                },
            })
            .map_err(EngineError::from)
            .boxed()
    }

    async fn create_container(&self, spec: &CreateSpec) -> EngineResult<String> {
        let options = spec
            .name
            .as_ref()
            .map(|name| q::CreateContainerOptionsBuilder::new().name(name).build());
        let created = self
            .docker
            .create_container(options, create_body(spec))
            .await?;
        for warning in &created.warnings {
            tracing::warn!(%warning, "create container");
        }
        Ok(created.id)
    }

    async fn images(&self) -> EngineResult<Vec<Image>> {
        let list = self
            .docker
            .list_images(Some(q::ListImagesOptionsBuilder::new().all(false).build()))
            .await?;
        Ok(list.into_iter().map(convert::image).collect())
    }

    fn pull(&self, reference: &str) -> EngineStream<PullEvent> {
        let Some((image, tag)) = crate::pull::split_reference(reference) else {
            let error = EngineError::Api {
                status: 400,
                message: format!("Invalid image reference: {reference:?}"),
            };
            return futures::stream::once(async move { Err(error) }).boxed();
        };
        let options = q::CreateImageOptionsBuilder::new()
            .from_image(&image)
            .tag(&tag)
            .build();
        self.docker
            .create_image(Some(options), None, None)
            .map_ok(convert::pull_event)
            .map_err(EngineError::from)
            .boxed()
    }

    async fn remove_image(&self, id: &str) -> EngineResult<()> {
        self.docker
            .remove_image(id, None::<q::RemoveImageOptions>, None)
            .await?;
        Ok(())
    }

    async fn tag_image(&self, id: &str, repo: &str, tag: &str) -> EngineResult<()> {
        let options = q::TagImageOptionsBuilder::new().repo(repo).tag(tag).build();
        self.docker.tag_image(id, Some(options)).await?;
        Ok(())
    }

    async fn image_history(&self, id: &str) -> EngineResult<Vec<ImageLayer>> {
        let history = self.docker.image_history(id).await?;
        Ok(history.into_iter().map(convert::image_layer).collect())
    }

    async fn prune_images(&self) -> EngineResult<PruneReport> {
        let options = q::PruneImagesOptions {
            filters: label_filter("dangling", "true"),
        };
        let r = self.docker.prune_images(Some(options)).await?;
        let deleted = r
            .images_deleted
            .unwrap_or_default()
            .into_iter()
            .filter_map(|i| i.deleted.or(i.untagged))
            .collect();
        Ok(PruneReport {
            deleted,
            reclaimed: convert::size(r.space_reclaimed),
        })
    }

    async fn volumes(&self) -> EngineResult<Vec<Volume>> {
        let list = self
            .docker
            .list_volumes(None::<q::ListVolumesOptions>)
            .await?;
        let mut volumes: Vec<Volume> = list
            .volumes
            .unwrap_or_default()
            .into_iter()
            .map(convert::volume)
            .collect();
        // Sizes come from `/system/df?type=volume` (best effort; can be slow on big volumes).
        match rawhttp::get(&self.endpoint, "/system/df?type=volume", DF_TIMEOUT).await {
            Ok((200, body)) => match serde_json::from_slice::<serde_json::Value>(&body) {
                Ok(df) => convert::merge_volume_usage(&mut volumes, &df),
                Err(error) => tracing::debug!(%error, "volume usage: invalid JSON"),
            },
            Ok((status, _)) => tracing::debug!(status, "volume usage unavailable"),
            Err(error) => tracing::debug!(%error, "volume usage unavailable"),
        }
        volumes.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(volumes)
    }

    async fn remove_volume(&self, name: &str) -> EngineResult<()> {
        self.docker
            .remove_volume(name, None::<q::RemoveVolumeOptions>)
            .await?;
        Ok(())
    }

    async fn prune_volumes(&self) -> EngineResult<PruneReport> {
        // `all=true`: named volumes too (API ≥ 1.42 only prunes anonymous ones by default).
        let options = q::PruneVolumesOptions {
            filters: label_filter("all", "true"),
        };
        let r = self.docker.prune_volumes(Some(options)).await?;
        Ok(PruneReport {
            deleted: r.volumes_deleted.unwrap_or_default(),
            reclaimed: convert::size(r.space_reclaimed),
        })
    }

    async fn networks(&self) -> EngineResult<Vec<Network>> {
        let list = self
            .docker
            .list_networks(None::<q::ListNetworksOptions>)
            .await?;
        let mut networks: Vec<Network> = list.into_iter().map(convert::network).collect();
        networks.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(networks)
    }

    async fn remove_network(&self, id: &str) -> EngineResult<()> {
        self.docker.remove_network(id).await?;
        Ok(())
    }

    async fn prune_networks(&self) -> EngineResult<PruneReport> {
        let r = self
            .docker
            .prune_networks(None::<q::PruneNetworksOptions>)
            .await?;
        Ok(PruneReport {
            deleted: r.networks_deleted.unwrap_or_default(),
            reclaimed: 0,
        })
    }
}

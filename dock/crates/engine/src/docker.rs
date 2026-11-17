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
use crate::registry::{AuthOutcome, RegistryAuth};
use crate::tunnel::{self, SshTarget, Tunnel};
use async_trait::async_trait;
use bollard::Docker;
use bollard::auth::DockerCredentials;
use bollard::container::LogOutput;
use bollard::query_parameters as q;
use futures::{StreamExt, TryStreamExt};
use std::collections::HashMap;
use std::sync::Arc;

/// Seconds before a request without a response fails (streams are not cut).
const TIMEOUT_SECS: u64 = 30;
const DF_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
/// The engine contacts the registry, which can be slow.
const AUTH_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(45);

#[derive(Clone)]
pub struct DockerEngine {
    docker: Docker,
    /// Where requests go: the engine's socket or address, or the local end
    /// of the SSH tunnel for an `ssh://` engine.
    endpoint: Endpoint,
    /// Kept open as long as the engine (and its clones) live.
    _tunnel: Option<Arc<Tunnel>>,
}

impl DockerEngine {
    /// Connects to `endpoint` and negotiates the API version with the engine.
    /// An `ssh://` endpoint first gets an SSH tunnel to the remote socket.
    pub async fn connect(endpoint: &Endpoint) -> EngineResult<Self> {
        let (local, tunnel) = match endpoint {
            Endpoint::Ssh(target) => {
                let parsed = SshTarget::parse(target).ok_or_else(|| EngineError::Unreachable {
                    endpoint: endpoint.to_string(),
                    message: "invalid ssh:// address".into(),
                })?;
                let ssh = tunnel::find_ssh().ok_or_else(|| EngineError::Unreachable {
                    endpoint: endpoint.to_string(),
                    message: "ssh was not found".into(),
                })?;
                let tunnel = Tunnel::open(&ssh, &parsed).await?;
                (
                    Endpoint::Unix(tunnel.socket.clone()),
                    Some(Arc::new(tunnel)),
                )
            }
            other => (other.clone(), None),
        };
        let unreachable = |e: bollard::errors::Error| EngineError::Unreachable {
            endpoint: endpoint.to_string(),
            message: if tunnel.is_some() {
                format!(
                    "{e} (through SSH to {}: is the engine running there, and may this user use it?)",
                    tunnel::REMOTE_SOCKET
                )
            } else {
                e.to_string()
            },
        };
        let docker = match &local {
            Endpoint::Unix(path) => Docker::connect_with_unix(
                &path.to_string_lossy(),
                TIMEOUT_SECS,
                bollard::API_DEFAULT_VERSION,
            ),
            Endpoint::Tcp(addr) => {
                Docker::connect_with_http(addr, TIMEOUT_SECS, bollard::API_DEFAULT_VERSION)
            }
            // Replaced by the tunnel's socket above.
            Endpoint::Ssh(_) => {
                return Err(EngineError::Unreachable {
                    endpoint: endpoint.to_string(),
                    message: "no SSH tunnel".into(),
                });
            }
        }
        .map_err(unreachable)?;
        let docker = docker.negotiate_version().await.map_err(unreachable)?;
        Ok(Self {
            docker,
            endpoint: local,
            _tunnel: tunnel,
        })
    }
}

fn label_filter(key: &str, value: &str) -> Option<HashMap<String, Vec<String>>> {
    Some(HashMap::from([(key.to_owned(), vec![value.to_owned()])]))
}

/// The `X-Registry-Auth` credentials of a pull: an identity token, or user and password.
fn credentials(auth: &RegistryAuth) -> DockerCredentials {
    if auth.is_identity_token() {
        DockerCredentials {
            identitytoken: Some(auth.secret.clone()),
            serveraddress: Some(auth.server.clone()),
            ..DockerCredentials::default()
        }
    } else {
        DockerCredentials {
            username: Some(auth.username.clone()),
            password: Some(auth.secret.clone()),
            serveraddress: Some(auth.server.clone()),
            ..DockerCredentials::default()
        }
    }
}

/// The `POST /auth` body (the engine's `AuthConfig`).
fn auth_body(auth: &RegistryAuth) -> serde_json::Value {
    if auth.is_identity_token() {
        serde_json::json!({ "identitytoken": auth.secret, "serveraddress": auth.server })
    } else {
        serde_json::json!({
            "username": auth.username,
            "password": auth.secret,
            "serveraddress": auth.server,
        })
    }
}

/// `200 {"Status","IdentityToken"}`, or the engine's error message (401 for bad credentials).
fn parse_auth_response(status: u16, body: &[u8]) -> EngineResult<AuthOutcome> {
    let value: serde_json::Value = serde_json::from_slice(body).unwrap_or_default();
    let text = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_str)
            .filter(|s| !s.is_empty())
            .map(str::to_owned)
    };
    if status == 200 {
        return Ok(AuthOutcome {
            status: text("Status").unwrap_or_else(|| "Login Succeeded".to_owned()),
            identity_token: text("IdentityToken"),
        });
    }
    let message = text("message").unwrap_or_else(|| {
        let raw = String::from_utf8_lossy(body).trim().to_owned();
        if raw.is_empty() {
            format!("the engine answered {status}")
        } else {
            raw
        }
    });
    Err(EngineError::Api { status, message })
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

    fn pull(&self, reference: &str, auth: Option<RegistryAuth>) -> EngineStream<PullEvent> {
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
            .create_image(Some(options), None, auth.as_ref().map(credentials))
            .map_ok(convert::pull_event)
            .map_err(EngineError::from)
            .boxed()
    }

    async fn check_auth(&self, auth: &RegistryAuth) -> EngineResult<AuthOutcome> {
        let body = auth_body(auth).to_string();
        let (status, response) =
            rawhttp::post_json(&self.endpoint, "/auth", body.as_bytes(), AUTH_TIMEOUT).await?;
        parse_auth_response(status, &response)
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

#[cfg(test)]
mod tests {
    use super::*;

    fn auth(username: &str) -> RegistryAuth {
        RegistryAuth {
            server: "ghcr.io".into(),
            username: username.into(),
            secret: "s3cret".into(),
        }
    }

    #[test]
    fn sends_a_password_or_an_identity_token() {
        let body = auth_body(&auth("me"));
        assert_eq!(body["username"], "me");
        assert_eq!(body["password"], "s3cret");
        assert_eq!(body["serveraddress"], "ghcr.io");
        let token = auth_body(&auth(crate::registry::TOKEN_USERNAME));
        assert_eq!(token["identitytoken"], "s3cret");
        assert!(token.get("password").is_none());
        let creds = credentials(&auth(crate::registry::TOKEN_USERNAME));
        assert_eq!(creds.identitytoken.as_deref(), Some("s3cret"));
        assert!(creds.password.is_none());
    }

    #[test]
    fn reads_the_auth_answer() {
        let ok = parse_auth_response(200, br#"{"Status":"Login Succeeded","IdentityToken":""}"#);
        assert_eq!(
            ok,
            Ok(AuthOutcome {
                status: "Login Succeeded".into(),
                identity_token: None
            })
        );
        let token = parse_auth_response(200, br#"{"Status":"ok","IdentityToken":"tok"}"#);
        assert_eq!(
            token.ok().and_then(|o| o.identity_token).as_deref(),
            Some("tok")
        );
        let refused = parse_auth_response(
            401,
            br#"{"message":"login attempt to https://ghcr.io/v2/ failed with status: 401 Unauthorized"}"#,
        );
        assert!(
            matches!(refused, Err(EngineError::Api { status: 401, message }) if message.contains("401 Unauthorized"))
        );
        assert!(parse_auth_response(500, b"").is_err_and(|e| e.to_string().contains("500")));
    }
}

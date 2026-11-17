//! In-memory [`Engine`] for tests: behaves like a small Docker engine
//! (state transitions, 404/409 errors, prune semantics) and records calls.

use crate::archive::{EntryKind, TarEvent, TarReader};
use crate::create::CreateSpec;
use crate::engine::{Engine, EngineStream};
use crate::error::{EngineError, EngineResult};
use crate::model::{
    COMPOSE_PROJECT, COMPOSE_SERVICE, Container, ContainerAction, ContainerDetail, ContainerState,
    EngineInfo, ExecOutput, Image, ImageLayer, Labels, LogChunk, LogOptions, Network, PruneReport,
    PullEvent, Stats, Volume,
};
use crate::registry::{AuthOutcome, RegistryAuth};
use async_trait::async_trait;
use futures::StreamExt;
use std::collections::{BTreeMap, HashMap};
use std::sync::{Mutex, MutexGuard, PoisonError};

#[derive(Debug, Default)]
pub struct FakeState {
    pub down: bool,
    pub containers: Vec<Container>,
    pub images: Vec<Image>,
    pub volumes: Vec<Volume>,
    pub networks: Vec<Network>,
    /// Logs of any container without its own entry in `container_logs`.
    pub logs: Vec<LogChunk>,
    pub container_logs: HashMap<String, Vec<LogChunk>>,
    /// Options of every `logs` call, in order.
    pub log_requests: Vec<(String, LogOptions)>,
    pub pull_events: Vec<PullEvent>,
    /// Accounts the fake's registries accept: store key → (username, secret).
    pub registry_accounts: HashMap<String, (String, String)>,
    /// Repositories (`ghcr.io/org/app`) that refuse anonymous pulls.
    pub private_repos: Vec<String>,
    /// Container file systems: id → absolute path → (kind, file data or link target).
    pub files: HashMap<String, BTreeMap<String, (EntryKind, Vec<u8>)>>,
    /// Every archive download, `<id> <path>`.
    pub archive_requests: Vec<String>,
    /// Containers without `sh` (distroless): an exec of it exits 127.
    pub no_shell: Vec<String>,
    /// Every exec, `<id> <first argument>`.
    pub execs: Vec<String>,
    /// Every mutating call, e.g. `remove container web force=true`.
    pub calls: Vec<String>,
}

impl FakeState {
    /// What [`crate::files::LIST_SCRIPT`] would print for `dir`, or exit 2.
    fn list_output(&self, id: &str, dir: &str) -> ExecOutput {
        let fs = self.files.get(id).cloned().unwrap_or_default();
        let dir = crate::files::normalize(dir);
        if dir != "/" && fs.get(&dir).is_none_or(|(k, _)| *k != EntryKind::Dir) {
            return ExecOutput {
                exit_code: Some(2),
                ..ExecOutput::default()
            };
        }
        let mut stdout = Vec::new();
        for (path, (kind, data)) in &fs {
            if crate::files::parent(path) != dir || path == &dir {
                continue;
            }
            let (t, link) = match kind {
                EntryKind::Dir => ("d", String::new()),
                EntryKind::File => ("f", String::new()),
                EntryKind::Symlink => ("l", String::from_utf8_lossy(data).into_owned()),
                EntryKind::Other => ("o", String::new()),
            };
            let mode = match kind {
                EntryKind::Dir => "755",
                EntryKind::Symlink => "777",
                _ => "644",
            };
            let name = crate::files::base_name(path);
            stdout.extend_from_slice(
                format!("{t}\t{} {mode} 1700000000\t{link}\t{name}\0", data.len()).as_bytes(),
            );
        }
        ExecOutput {
            stdout,
            stderr: String::new(),
            exit_code: Some(0),
        }
    }

    /// The archive the engine would send for `path`: the entry itself,
    /// named by its last part (`./` for `/`), then everything below it.
    fn archive_of(&self, id: &str, path: &str) -> EngineResult<Vec<u8>> {
        if !self.containers.iter().any(|c| c.id == id) {
            return Err(not_found("container", id));
        }
        let path = crate::files::normalize(path);
        let fs = self.files.get(id).cloned().unwrap_or_default();
        let (root_name, prefix) = if path == "/" {
            ("./".to_owned(), "/".to_owned())
        } else {
            let (kind, _) = fs.get(&path).ok_or_else(|| EngineError::Api {
                status: 404,
                message: format!("Could not find the file {path} in container {id}"),
            })?;
            let name = crate::files::base_name(&path);
            let name = if *kind == EntryKind::Dir {
                format!("{name}/")
            } else {
                name
            };
            (name, format!("{path}/"))
        };
        let base = root_name.trim_end_matches('/').to_owned();
        let mut names: Vec<(String, EntryKind, Vec<u8>)> = Vec::new();
        if path == "/" {
            names.push((root_name, EntryKind::Dir, Vec::new()));
        } else if let Some((kind, data)) = fs.get(&path) {
            names.push((root_name, *kind, data.clone()));
        }
        for (p, (kind, data)) in fs.range(prefix.clone()..) {
            let Some(rel) = p.strip_prefix(&prefix) else {
                break;
            };
            let name = if base == "." {
                format!("./{rel}")
            } else {
                format!("{base}/{rel}")
            };
            let name = if *kind == EntryKind::Dir {
                format!("{name}/")
            } else {
                name
            };
            names.push((name, *kind, data.clone()));
        }
        let entries: Vec<(&str, EntryKind, &[u8])> = names
            .iter()
            .map(|(n, k, d)| (n.as_str(), *k, d.as_slice()))
            .collect();
        Ok(crate::archive::build(&entries))
    }

    /// Whether a registry account matches `auth`.
    fn accepts(&self, auth: &RegistryAuth) -> bool {
        self.registry_accounts
            .get(&auth.server)
            .is_some_and(|(user, secret)| *user == auth.username && *secret == auth.secret)
    }
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

/// `nginx` → `nginx:latest`, the way the engine stores tags.
fn with_tag(reference: &str) -> String {
    crate::pull::split_reference(reference).map_or_else(
        || reference.to_owned(),
        |(name, tag)| format!("{name}:{tag}"),
    )
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

    /// Adds files to container `id`: (absolute path, kind, data or link target).
    pub fn add_files(&self, id: &str, files: &[(&str, EntryKind, &[u8])]) {
        let mut s = self.state();
        let fs = s.files.entry(id.to_owned()).or_default();
        for (path, kind, data) in files {
            fs.insert((*path).to_owned(), (*kind, data.to_vec()));
        }
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
                net_rx: 2_048,
                net_tx: 1_024,
                blk_read: 4_096,
                blk_write: 0,
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

    fn logs(&self, id: &str, options: LogOptions) -> EngineStream<LogChunk> {
        let mut s = self.state();
        s.log_requests.push((id.to_owned(), options));
        let chunks = s.container_logs.get(id).unwrap_or(&s.logs).clone();
        futures::stream::iter(chunks.into_iter().map(Ok)).boxed()
    }

    async fn create_container(&self, spec: &CreateSpec) -> EngineResult<String> {
        let mut s = self.guard()?;
        let image = with_tag(&spec.image);
        s.calls.push(format!(
            "create {} {image}",
            spec.name.as_deref().unwrap_or("-")
        ));
        if !s.images.iter().any(|i| i.tags.contains(&image)) {
            return Err(not_found("image", &image));
        }
        let id = format!("new{:03}", s.containers.len() + 1);
        let name = spec.name.clone().unwrap_or_else(|| format!("auto_{id}"));
        if s.containers.iter().any(|c| c.name == name) {
            return Err(conflict(format!(
                "Conflict. The container name \"/{name}\" is already in use"
            )));
        }
        let mut c = container(&id, &name, None, "created");
        c.image = image;
        s.containers.push(c);
        Ok(id)
    }

    fn archive(&self, id: &str, path: &str) -> EngineStream<Vec<u8>> {
        let mut s = self.state();
        s.archive_requests.push(format!("{id} {path}"));
        match s.archive_of(id, path) {
            // Sent in small pieces, like a socket would.
            Ok(tar) => {
                futures::stream::iter(tar.chunks(700).map(|c| Ok(c.to_vec())).collect::<Vec<_>>())
                    .boxed()
            }
            Err(error) => futures::stream::once(async move { Err(error) }).boxed(),
        }
    }

    async fn put_archive(&self, id: &str, dir: &str, tar: Vec<u8>) -> EngineResult<()> {
        let mut s = self.guard()?;
        if !s.containers.iter().any(|c| c.id == id) {
            return Err(not_found("container", id));
        }
        let dir = crate::files::normalize(dir);
        let fs = s.files.entry(id.to_owned()).or_default();
        if dir != "/" && fs.get(&dir).is_none_or(|(k, _)| *k != EntryKind::Dir) {
            return Err(EngineError::Api {
                status: 404,
                message: format!("Could not find the file {dir} in container {id}"),
            });
        }
        let mut files: Vec<(String, EntryKind, Vec<u8>)> = Vec::new();
        TarReader::default()
            .push(&tar, &mut |event| match event {
                TarEvent::Entry(e) => files.push((e.name, e.kind, Vec::new())),
                TarEvent::Data(d) => {
                    if let Some(last) = files.last_mut() {
                        last.2.extend_from_slice(d);
                    }
                }
            })
            .map_err(|e| EngineError::Api {
                status: 400,
                message: e,
            })?;
        let mut uploaded = Vec::new();
        for (name, kind, data) in files {
            let path = crate::files::join(&dir, &name);
            fs.insert(path.clone(), (kind, data));
            uploaded.push(path);
        }
        for path in uploaded {
            s.calls.push(format!("upload {id} {path}"));
        }
        Ok(())
    }

    async fn exec_capture(&self, id: &str, cmd: &[String]) -> EngineResult<ExecOutput> {
        let mut s = self.guard()?;
        s.execs
            .push(format!("{id} {}", cmd.first().map_or("", String::as_str)));
        let container = s
            .containers
            .iter()
            .find(|c| c.id == id)
            .ok_or_else(|| not_found("container", id))?;
        if !container.state.is_up() {
            return Err(conflict(format!("Container {id} is not running")));
        }
        if s.no_shell.iter().any(|c| c == id) {
            return Ok(ExecOutput {
                stdout: Vec::new(),
                stderr: "exec: \"sh\": executable file not found in $PATH".into(),
                exit_code: Some(127),
            });
        }
        let listing = cmd
            .get(2)
            .is_some_and(|script| script == crate::files::LIST_SCRIPT);
        Ok(match cmd.get(4) {
            Some(dir) if listing => s.list_output(id, dir),
            _ => ExecOutput {
                exit_code: Some(0),
                ..ExecOutput::default()
            },
        })
    }

    async fn images(&self) -> EngineResult<Vec<Image>> {
        Ok(self.guard()?.images.clone())
    }

    fn pull(&self, reference: &str, auth: Option<RegistryAuth>) -> EngineStream<PullEvent> {
        let mut s = self.state();
        s.calls.push(match &auth {
            Some(a) => format!("pull {reference} as {}@{}", a.username, a.server),
            None => format!("pull {reference}"),
        });
        let repo = crate::pull::split_reference(reference).map_or_else(String::new, |(r, _)| r);
        if s.private_repos.contains(&repo) && !auth.as_ref().is_some_and(|a| s.accepts(a)) {
            let error = EngineError::Api {
                status: 404,
                message: format!(
                    "pull access denied for {repo}, repository does not exist or may require 'docker login'"
                ),
            };
            return futures::stream::once(async move { Err(error) }).boxed();
        }
        let events = s.pull_events.clone();
        let tag = with_tag(reference);
        if events.iter().all(|e| e.error.is_none())
            && !s.images.iter().any(|i| i.tags.contains(&tag))
        {
            let id = format!("sha256:pulled{}", s.images.len());
            s.images.push(Image {
                id,
                tags: vec![tag],
                size: 10,
                created: 0,
                containers: 0,
            });
        }
        futures::stream::iter(events.into_iter().map(Ok)).boxed()
    }

    async fn check_auth(&self, auth: &RegistryAuth) -> EngineResult<AuthOutcome> {
        let mut s = self.guard()?;
        s.calls
            .push(format!("auth {} {}", auth.server, auth.username));
        if s.accepts(auth) {
            Ok(AuthOutcome {
                status: "Login Succeeded".into(),
                identity_token: None,
            })
        } else {
            Err(EngineError::Api {
                status: 401,
                message: format!(
                    "login attempt to {} failed with status: 401 Unauthorized",
                    auth.server
                ),
            })
        }
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

    async fn tag_image(&self, id: &str, repo: &str, tag: &str) -> EngineResult<()> {
        let mut s = self.guard()?;
        s.calls.push(format!("tag {id} {repo}:{tag}"));
        let reference = format!("{repo}:{tag}");
        for image in &mut s.images {
            image.tags.retain(|t| *t != reference);
        }
        let image = s
            .images
            .iter_mut()
            .find(|i| i.id == id)
            .ok_or_else(|| not_found("image", id))?;
        image.tags.push(reference);
        Ok(())
    }

    async fn image_history(&self, id: &str) -> EngineResult<Vec<ImageLayer>> {
        let s = self.guard()?;
        let image = s
            .images
            .iter()
            .find(|i| i.id == id)
            .ok_or_else(|| not_found("image", id))?;
        let layer = |id: &str, by: &str, size| ImageLayer {
            id: id.to_owned(),
            created: 0,
            created_by: by.to_owned(),
            size,
            tags: Vec::new(),
            comment: String::new(),
        };
        let mut top = layer(&image.id, "CMD [\"app\"]", 0);
        top.tags.clone_from(&image.tags);
        Ok(vec![
            top,
            layer("<missing>", "COPY . /app", image.size / 2),
            layer(
                "<missing>",
                "/bin/sh -c #(nop) ADD file:base in /",
                image.size / 2,
            ),
        ])
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

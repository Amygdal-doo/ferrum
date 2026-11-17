//! Engine-neutral models: what the UI shows, independent of the bollard types.

use std::collections::BTreeMap;

/// Compose labels set by `docker compose` on every container / volume / network it creates.
pub const COMPOSE_PROJECT: &str = "com.docker.compose.project";
pub const COMPOSE_SERVICE: &str = "com.docker.compose.service";
pub const COMPOSE_WORKING_DIR: &str = "com.docker.compose.project.working_dir";
pub const COMPOSE_CONFIG_FILES: &str = "com.docker.compose.project.config_files";

pub type Labels = BTreeMap<String, String>;

/// What the engine reports about itself (`GET /version`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct EngineInfo {
    pub version: String,
    pub api_version: String,
    pub os: String,
    pub arch: String,
    /// e.g. "Docker Desktop 4.40.0 (187762)" or "Docker Engine - Community".
    pub platform: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContainerState {
    Created,
    Running,
    Paused,
    Restarting,
    Exited,
    Removing,
    Dead,
    Unknown,
}

impl ContainerState {
    #[must_use]
    pub fn parse(state: &str) -> Self {
        match state.trim().to_ascii_lowercase().as_str() {
            "created" => Self::Created,
            "running" => Self::Running,
            "paused" => Self::Paused,
            "restarting" => Self::Restarting,
            "exited" | "stopped" => Self::Exited,
            "removing" => Self::Removing,
            "dead" => Self::Dead,
            _ => Self::Unknown,
        }
    }

    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Self::Created => "created",
            Self::Running => "running",
            Self::Paused => "paused",
            Self::Restarting => "restarting",
            Self::Exited => "exited",
            Self::Removing => "removing",
            Self::Dead => "dead",
            Self::Unknown => "unknown",
        }
    }

    /// Running or restarting: has a live process (stats, exec, stop).
    #[must_use]
    pub fn is_up(self) -> bool {
        matches!(self, Self::Running | Self::Restarting)
    }

    #[must_use]
    pub fn can_start(self) -> bool {
        matches!(self, Self::Created | Self::Exited | Self::Dead)
    }

    #[must_use]
    pub fn can_stop(self) -> bool {
        matches!(self, Self::Running | Self::Restarting | Self::Paused)
    }

    #[must_use]
    pub fn can_pause(self) -> bool {
        self == Self::Running
    }

    #[must_use]
    pub fn can_unpause(self) -> bool {
        self == Self::Paused
    }
}

/// One published (or merely exposed) port of a container.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PortMapping {
    /// Host IP the port is bound to (`0.0.0.0`, `::`, `127.0.0.1`…), if published.
    pub ip: Option<String>,
    pub private: u16,
    pub public: Option<u16>,
    /// `tcp`, `udp` or `sctp`.
    pub protocol: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Container {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    /// Human status from the engine, e.g. "Up 3 hours (healthy)".
    pub status: String,
    pub ports: Vec<PortMapping>,
    pub labels: Labels,
    /// Unix seconds.
    pub created: i64,
}

impl Container {
    /// Compose project name, if the container was created by `docker compose`.
    #[must_use]
    pub fn project(&self) -> Option<&str> {
        self.labels.get(COMPOSE_PROJECT).map(String::as_str)
    }

    #[must_use]
    pub fn service(&self) -> Option<&str> {
        self.labels.get(COMPOSE_SERVICE).map(String::as_str)
    }

    #[must_use]
    pub fn short_id(&self) -> &str {
        crate::format::short_id(&self.id)
    }
}

/// One stats sample. Network and block I/O are cumulative counters since
/// the container started (rates come from two samples, see `stats::IoRates`).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    /// 100 % = one full core (like `docker stats`).
    pub cpu_percent: f64,
    pub mem_usage: u64,
    pub mem_limit: u64,
    /// Bytes received / sent over all interfaces.
    pub net_rx: u64,
    pub net_tx: u64,
    /// Bytes read from / written to block devices.
    pub blk_read: u64,
    pub blk_write: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mount {
    /// `bind`, `volume`, `tmpfs`…
    pub kind: String,
    /// Host path or volume name.
    pub source: String,
    pub destination: String,
    pub read_write: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NetworkAttachment {
    pub network: String,
    pub ip_address: String,
    pub gateway: String,
    pub aliases: Vec<String>,
}

/// `docker inspect` reduced to what the detail panel shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ContainerDetail {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    pub created: String,
    pub started_at: Option<String>,
    pub command: String,
    pub working_dir: Option<String>,
    pub tty: bool,
    pub restart_count: i64,
    pub env: Vec<(String, String)>,
    pub mounts: Vec<Mount>,
    pub networks: Vec<NetworkAttachment>,
    pub labels: Labels,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub id: String,
    /// `repo:tag` references; empty for dangling images.
    pub tags: Vec<String>,
    pub size: u64,
    /// Unix seconds.
    pub created: i64,
    /// Containers using it, or -1 when the engine did not count.
    pub containers: i64,
}

impl Image {
    /// Untagged (`<none>:<none>`) image.
    #[must_use]
    pub fn is_dangling(&self) -> bool {
        self.tags
            .iter()
            .all(|t| t == "<none>:<none>" || t.is_empty())
    }

    #[must_use]
    pub fn short_id(&self) -> &str {
        crate::format::short_id(&self.id)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Volume {
    pub name: String,
    pub driver: String,
    pub mountpoint: String,
    pub created: Option<String>,
    pub labels: Labels,
    /// Only known when the engine reports usage data.
    pub size: Option<u64>,
    pub ref_count: Option<i64>,
}

impl Volume {
    #[must_use]
    pub fn project(&self) -> Option<&str> {
        self.labels.get(COMPOSE_PROJECT).map(String::as_str)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Network {
    pub id: String,
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub internal: bool,
    pub created: Option<String>,
    pub labels: Labels,
}

impl Network {
    /// Networks every engine ships with; they cannot be removed.
    #[must_use]
    pub fn is_builtin(&self) -> bool {
        matches!(self.name.as_str(), "bridge" | "host" | "none")
    }

    #[must_use]
    pub fn project(&self) -> Option<&str> {
        self.labels.get(COMPOSE_PROJECT).map(String::as_str)
    }
}

/// What a command run with `exec` printed, and how it exited.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecOutput {
    pub stdout: Vec<u8>,
    pub stderr: String,
    /// `None` when the engine did not report one.
    pub exit_code: Option<i64>,
}

/// Result of a prune: what was removed and how much disk came back.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PruneReport {
    pub deleted: Vec<String>,
    pub reclaimed: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LogStream {
    Stdout,
    Stderr,
}

/// A raw piece of log output; may contain several lines or end mid-line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LogChunk {
    pub stream: LogStream,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LogOptions {
    pub follow: bool,
    /// Last N lines; `None` = everything.
    pub tail: Option<u32>,
    pub timestamps: bool,
    /// Only lines after this Unix time (seconds); `None` = from the start.
    pub since: Option<i64>,
}

impl Default for LogOptions {
    fn default() -> Self {
        Self {
            follow: true,
            tail: Some(1000),
            timestamps: true,
            since: None,
        }
    }
}

/// One layer of `docker image history` (newest first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageLayer {
    /// Layer image id, or `<missing>` for layers built elsewhere.
    pub id: String,
    /// Unix seconds.
    pub created: i64,
    /// The Dockerfile instruction, e.g. `/bin/sh -c #(nop) CMD ["nginx"]`.
    pub created_by: String,
    pub size: u64,
    pub tags: Vec<String>,
    pub comment: String,
}

/// One progress message of `POST /images/create`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PullEvent {
    /// Layer id, when the message is about a layer.
    pub id: Option<String>,
    pub status: String,
    pub current: Option<u64>,
    pub total: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainerAction {
    Start,
    Stop,
    Restart,
    Pause,
    Unpause,
    /// `force` also kills a running container.
    Remove {
        force: bool,
    },
}

impl ContainerAction {
    #[must_use]
    pub fn verb(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Stop => "stop",
            Self::Restart => "restart",
            Self::Pause => "pause",
            Self::Unpause => "unpause",
            Self::Remove { .. } => "remove",
        }
    }

    #[must_use]
    pub fn past_tense(self) -> &'static str {
        match self {
            Self::Start => "Started",
            Self::Stop => "Stopped",
            Self::Restart => "Restarted",
            Self::Pause => "Paused",
            Self::Unpause => "Resumed",
            Self::Remove { .. } => "Removed",
        }
    }
}

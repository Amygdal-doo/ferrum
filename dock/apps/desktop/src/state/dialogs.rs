//! Dialog state: confirmations, pull, run, tag, image history, engine picker.

use ferrumdock_engine::compose::ComposeProject;
use ferrumdock_engine::context::DockerContext;
use ferrumdock_engine::create::RestartPolicy;
use ferrumdock_engine::credentials::SignedIn;
use ferrumdock_engine::pull::PullProgress;
use ferrumdock_engine::{Candidate, Container, Image, ImageLayer, Network};

/// What a confirm dialog does when accepted.
#[derive(Debug, Clone)]
pub enum PendingAction {
    RemoveContainer(Container),
    RemoveImage(Image),
    PruneImages,
    RemoveVolume(String),
    PruneVolumes,
    RemoveNetwork(Network),
    PruneNetworks,
    /// `docker context rm`.
    RemoveContext(String),
    /// `kubectl rollout restart deployment/<name>`.
    RestartDeployment {
        context: String,
        namespace: String,
        name: String,
    },
    /// Copy the local file into the container's folder.
    Upload {
        container: String,
        dir: String,
        local: std::path::PathBuf,
    },
    ComposeDown(ComposeProject),
}

#[derive(Debug, Clone)]
pub struct Confirm {
    pub title: String,
    pub body: String,
    pub confirm_label: String,
    pub action: PendingAction,
}

pub enum Dialog {
    Confirm(Box<Confirm>),
    Pull,
    /// "Run container" form.
    Run,
    /// New tag for this image.
    Tag(Image),
    /// Layers of an image.
    History,
    /// Engine / Docker context picker.
    Engines,
    /// Registry sign-in and the registries signed in to.
    Registries,
    /// Kubernetes context or namespace picker.
    Kube(crate::state::KubePicker),
    /// The logs of a pod (`state.kube.logs`).
    PodLogs,
}

/// A registry the sign-in form is filled for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RegistryPreset {
    #[default]
    DockerHub,
    Ghcr,
    Ecr,
    Other,
}

impl RegistryPreset {
    pub const ALL: [Self; 4] = [Self::DockerHub, Self::Ghcr, Self::Ecr, Self::Other];

    pub fn label(self) -> &'static str {
        match self {
            Self::DockerHub => "Docker Hub",
            Self::Ghcr => "GHCR",
            Self::Ecr => "Amazon ECR",
            Self::Other => "Other",
        }
    }

    /// (server, username) the form starts with.
    pub fn defaults(self) -> (&'static str, &'static str) {
        match self {
            Self::DockerHub => ("docker.io", ""),
            Self::Ghcr => ("ghcr.io", ""),
            Self::Ecr => ("", "AWS"),
            Self::Other => ("", ""),
        }
    }

    pub fn server_placeholder(self) -> &'static str {
        match self {
            Self::Ecr => "<account>.dkr.ecr.<region>.amazonaws.com",
            _ => "registry.example.com:5000",
        }
    }

    /// What goes in the password field.
    pub fn secret_hint(self) -> &'static str {
        match self {
            Self::DockerHub => "Password or a personal access token.",
            Self::Ghcr => "A personal access token (classic) with read:packages.",
            Self::Ecr => {
                "The token from `aws ecr get-login-password --region <region>` (valid for 12 hours)."
            }
            Self::Other => "Password or token.",
        }
    }
}

/// The registries dialog.
#[derive(Default)]
pub struct RegistryState {
    pub preset: RegistryPreset,
    pub signed_in: Option<Result<Vec<SignedIn>, String>>,
    /// Signing in or out.
    pub busy: bool,
    pub error: Option<String>,
}

/// The run dialog's non-text fields and outcome.
#[derive(Default)]
pub struct RunState {
    pub restart: RestartPolicy,
    /// Off: open the container's live logs once it starts (the GUI's "attach").
    pub detached: bool,
    pub running: bool,
    pub status: Option<String>,
    pub error: Option<String>,
}

/// `docker image history` of one image.
pub struct LayersState {
    pub label: String,
    pub layers: Option<Result<Vec<ImageLayer>, String>>,
}

/// Engines the picker offers.
#[derive(Default)]
pub struct EnginePicker {
    /// Engines present on this machine, then remote (`ssh://`) contexts.
    pub candidates: Vec<Candidate>,
    /// Every Docker context (`docker context ls`), the CLI's current one marked.
    pub contexts: Vec<DockerContext>,
    /// Docker contexts FerrumContainers cannot use (npipe, TLS…), listed for clarity.
    pub unsupported: Vec<DockerContext>,
    pub loading: bool,
    /// A `docker context` command is running.
    pub busy: bool,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct PullState {
    pub reference: String,
    pub running: bool,
    pub progress: PullProgress,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdock_engine::registry::{DOCKER_HUB, server_key};

    #[test]
    fn registry_presets_fill_the_store_keys_docker_login_uses() {
        assert_eq!(
            server_key(RegistryPreset::DockerHub.defaults().0),
            DOCKER_HUB
        );
        assert_eq!(server_key(RegistryPreset::Ghcr.defaults().0), "ghcr.io");
        // ECR: the account's registry is typed; the username is always AWS.
        assert_eq!(RegistryPreset::Ecr.defaults(), ("", "AWS"));
        assert!(
            RegistryPreset::Ecr
                .secret_hint()
                .contains("get-login-password")
        );
        assert_eq!(RegistryPreset::default(), RegistryPreset::DockerHub);
    }
}

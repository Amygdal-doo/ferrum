//! Dialog state: confirmations, pull, run, tag, image history, engine picker.

use ferrumdock_engine::compose::ComposeProject;
use ferrumdock_engine::context::DockerContext;
use ferrumdock_engine::create::RestartPolicy;
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
    pub candidates: Vec<Candidate>,
    /// Docker contexts FerrumDock cannot use (ssh…), listed for clarity.
    pub unsupported: Vec<DockerContext>,
    pub loading: bool,
}

#[derive(Default)]
pub struct PullState {
    pub reference: String,
    pub running: bool,
    pub progress: PullProgress,
    pub error: Option<String>,
}

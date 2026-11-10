//! UI state owned by the root view. Plain data plus pure helpers (tested);
//! the view mutates it and calls `cx.notify()`.

use ferrumdock_engine::compose::{self, ComposeProject, ContainerGroup};
use ferrumdock_engine::logs::LogBuffer;
use ferrumdock_engine::pull::PullProgress;
use ferrumdock_engine::{Connection, Container, ContainerDetail, Image, Network, Stats, Volume};
use std::collections::{HashMap, HashSet};

/// Lines kept per container log view.
pub const LOG_CAPACITY: usize = 20_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum Section {
    #[default]
    Containers,
    Images,
    Volumes,
    Networks,
}

impl Section {
    pub const ALL: [Self; 4] = [
        Self::Containers,
        Self::Images,
        Self::Volumes,
        Self::Networks,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Containers => "Containers",
            Self::Images => "Images",
            Self::Volumes => "Volumes",
            Self::Networks => "Networks",
        }
    }
}

pub enum EngineStatus {
    Connecting,
    Connected(Connection),
    Offline(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DetailTab {
    #[default]
    Logs,
    Inspect,
}

/// The container detail panel.
pub struct DetailState {
    pub id: String,
    pub tab: DetailTab,
    pub logs: LogBuffer,
    /// Logs are still streaming.
    pub streaming: bool,
    pub log_error: Option<String>,
    /// Keep the view scrolled to the newest line.
    pub follow: bool,
    pub timestamps: bool,
    /// Render ANSI colours (else show plain text).
    pub colors: bool,
    pub search: String,
    /// Line indexes matching `search`; `None` shows every line.
    pub matches: Option<Vec<usize>>,
    pub inspect: Option<Result<ContainerDetail, String>>,
}

impl DetailState {
    pub fn new(id: String) -> Self {
        Self {
            id,
            tab: DetailTab::Logs,
            logs: LogBuffer::new(LOG_CAPACITY, true),
            streaming: true,
            log_error: None,
            follow: true,
            timestamps: false,
            colors: true,
            search: String::new(),
            matches: None,
            inspect: None,
        }
    }

    /// Recomputes `matches` after the search text or the lines changed.
    pub fn refilter(&mut self) {
        self.matches = (!self.search.trim().is_empty()).then(|| self.logs.search(&self.search));
    }

    pub fn visible_count(&self) -> usize {
        self.matches.as_ref().map_or(self.logs.len(), Vec::len)
    }

    /// Buffer index of the `row`-th visible line.
    pub fn line_index(&self, row: usize) -> Option<usize> {
        match &self.matches {
            Some(m) => m.get(row).copied(),
            None => (row < self.logs.len()).then_some(row),
        }
    }
}

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub message: String,
    pub error: bool,
}

#[derive(Default)]
pub struct PullState {
    pub reference: String,
    pub running: bool,
    pub progress: PullProgress,
    pub error: Option<String>,
}

#[derive(Default)]
pub struct AppState {
    pub section: Section,
    pub containers: Vec<Container>,
    pub stats: HashMap<String, Stats>,
    pub images: Vec<Image>,
    pub volumes: Vec<Volume>,
    pub networks: Vec<Network>,
    /// Sections whose list has been fetched at least once since connecting.
    pub loaded: HashSet<Section>,
    pub filter: String,
    /// Collapsed compose project groups.
    pub collapsed: HashSet<String>,
    /// Ids / names with an operation in flight (buttons disabled).
    pub busy: HashSet<String>,
    pub detail: Option<DetailState>,
    pub dialog: Option<Dialog>,
    pub pull: Option<PullState>,
    pub notice: Option<Notice>,
}

impl AppState {
    /// Drops everything fetched from an engine (on disconnect / reconnect).
    pub fn clear_engine_data(&mut self) {
        self.containers.clear();
        self.stats.clear();
        self.images.clear();
        self.volumes.clear();
        self.networks.clear();
        self.loaded.clear();
        self.busy.clear();
        self.detail = None;
    }

    pub fn container(&self, id: &str) -> Option<&Container> {
        self.containers.iter().find(|c| c.id == id)
    }

    pub fn running_count(&self) -> usize {
        self.containers.iter().filter(|c| c.state.is_up()).count()
    }
}

/// One row of the containers list.
#[derive(Debug, Clone, PartialEq)]
pub enum Row {
    Group {
        project: String,
        running: usize,
        total: usize,
        collapsed: bool,
        compose: Option<ComposeProject>,
    },
    Container {
        container: Container,
        /// Indented under a compose group.
        grouped: bool,
    },
}

/// Flattens containers into grouped rows: filtered, compose projects first,
/// collapsed groups without their containers. A group stays visible when any
/// of its containers matches the filter (or the project name does).
pub fn container_rows(
    containers: &[Container],
    filter: &str,
    collapsed: &HashSet<String>,
) -> Vec<Row> {
    let needle = filter.trim().to_lowercase();
    let mut rows = Vec::new();
    for ContainerGroup {
        project,
        containers,
    } in compose::group_by_project(containers.to_vec())
    {
        let group = ContainerGroup {
            project: project.clone(),
            containers: containers.clone(),
        };
        let project_matches = project
            .as_deref()
            .is_some_and(|p| !needle.is_empty() && p.to_lowercase().contains(&needle));
        let visible: Vec<Container> = containers
            .into_iter()
            .filter(|c| project_matches || compose::matches_filter(c, filter))
            .collect();
        if visible.is_empty() {
            continue;
        }
        let grouped = project.is_some();
        if let Some(project) = project {
            let is_collapsed = collapsed.contains(&project);
            rows.push(Row::Group {
                running: group.running(),
                total: group.containers.len(),
                collapsed: is_collapsed,
                compose: group.compose_project(),
                project,
            });
            if is_collapsed {
                continue;
            }
        }
        rows.extend(
            visible
                .into_iter()
                .map(|container| Row::Container { container, grouped }),
        );
    }
    rows
}

/// Case-insensitive match of `filter` against any of `fields`.
pub fn matches_any(filter: &str, fields: &[&str]) -> bool {
    let needle = filter.trim().to_lowercase();
    needle.is_empty() || fields.iter().any(|f| f.to_lowercase().contains(&needle))
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumdock_engine::fake::container;

    fn sample() -> Vec<Container> {
        vec![
            container("1", "web-app-1", Some(("web", "app")), "running"),
            container("2", "web-db-1", Some(("web", "db")), "exited"),
            container("3", "scratch", None, "running"),
        ]
    }

    fn names(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Group {
                    project,
                    running,
                    total,
                    ..
                } => format!("[{project} {running}/{total}]"),
                Row::Container { container, .. } => container.name.clone(),
            })
            .collect()
    }

    #[test]
    fn groups_rows_by_project() {
        let rows = container_rows(&sample(), "", &HashSet::new());
        assert_eq!(
            names(&rows),
            ["[web 1/2]", "web-app-1", "web-db-1", "scratch"]
        );
        assert!(matches!(&rows[1], Row::Container { grouped: true, .. }));
        assert!(matches!(&rows[3], Row::Container { grouped: false, .. }));
    }

    #[test]
    fn collapses_and_filters() {
        let collapsed = HashSet::from(["web".to_owned()]);
        assert_eq!(
            names(&container_rows(&sample(), "", &collapsed)),
            ["[web 1/2]", "scratch"]
        );
        // Group counts stay those of the whole project while filtering.
        assert_eq!(
            names(&container_rows(&sample(), "db", &HashSet::new())),
            ["[web 1/2]", "web-db-1"]
        );
        assert_eq!(
            names(&container_rows(&sample(), "WEB", &HashSet::new())),
            ["[web 1/2]", "web-app-1", "web-db-1"]
        );
        assert!(container_rows(&sample(), "nothing", &HashSet::new()).is_empty());
    }

    #[test]
    fn matches_any_field() {
        assert!(matches_any("", &[]));
        assert!(matches_any("Gr", &["postgres"]));
        assert!(!matches_any("x", &["postgres", "local"]));
    }

    #[test]
    fn filters_log_lines() {
        use ferrumdock_engine::{LogChunk, LogStream};
        let mut detail = DetailState::new("1".into());
        detail.logs.push(&LogChunk {
            stream: LogStream::Stdout,
            text:
                "2024-05-01T10:00:00Z boot\n2024-05-01T10:00:01Z ERROR x\n2024-05-01T10:00:02Z ok\n"
                    .into(),
        });
        detail.refilter();
        assert_eq!(detail.visible_count(), 3);
        assert_eq!(detail.line_index(2), Some(2));
        assert_eq!(detail.line_index(3), None);
        detail.search = "error".into();
        detail.refilter();
        assert_eq!(detail.visible_count(), 1);
        assert_eq!(detail.line_index(0), Some(1));
    }

    #[test]
    fn clears_engine_data() {
        let mut state = AppState {
            containers: sample(),
            ..AppState::default()
        };
        state.loaded.insert(Section::Containers);
        state.detail = Some(DetailState::new("1".into()));
        assert_eq!(state.running_count(), 2);
        assert!(state.container("3").is_some());
        state.clear_engine_data();
        assert!(state.containers.is_empty() && state.loaded.is_empty() && state.detail.is_none());
    }
}

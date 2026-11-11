//! UI state owned by the root view. Plain data plus pure helpers (tested);
//! the view mutates it and calls `cx.notify()`.

mod detail;
mod dialogs;

pub use detail::{DetailState, DetailTab};
pub use dialogs::{Confirm, Dialog, EnginePicker, LayersState, PendingAction, PullState, RunState};

use crate::project::ProjectState;
use ferrumdock_engine::compose::{self, ComposeProject, ContainerGroup};
use ferrumdock_engine::project::ComposeFile;
use ferrumdock_engine::stats::StatsHistory;
use ferrumdock_engine::{Connection, Container, Endpoint, Image, Network, Stats, Volume};
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub message: String,
    pub error: bool,
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
    /// The folder open in the project bar.
    pub project: Option<ProjectState>,
    /// Recent stats per running container (sparklines, I/O rates).
    pub history: HashMap<String, StatsHistory>,
    pub run: RunState,
    pub layers: Option<LayersState>,
    pub engines: EnginePicker,
    /// Engine picked in the toolbar; tried first when (re)connecting.
    pub preferred: Option<Endpoint>,
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
        self.history.clear();
        self.detail = None;
    }

    /// Records a poll's stats at `at` seconds; containers without a sample lose their history.
    pub fn record_stats(&mut self, at: f64) {
        self.history.retain(|id, _| self.stats.contains_key(id));
        for (id, stats) in &self.stats {
            self.history.entry(id.clone()).or_default().push(at, *stats);
        }
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
    /// A compose file of the open project with no containers yet (offers Up).
    ComposeFile(ComposeFile),
}

/// Flattens containers into grouped rows: filtered, compose projects first,
/// collapsed groups without their containers. A group stays visible when any
/// of its containers matches the filter (or the project name does). With a
/// `scope` (project open, "this project only" on) only its containers are
/// listed; its compose files without containers come first either way.
pub fn container_rows(
    containers: &[Container],
    filter: &str,
    collapsed: &HashSet<String>,
    scope: Option<&ProjectState>,
) -> Vec<Row> {
    let needle = filter.trim().to_lowercase();
    let mut rows: Vec<Row> = scope
        .map(|p| p.idle_files(containers))
        .unwrap_or_default()
        .into_iter()
        .filter(|f| matches_any(filter, &[&f.project, &f.services.join(" ")]))
        .map(Row::ComposeFile)
        .collect();
    let containers: Vec<Container> = containers
        .iter()
        .filter(|c| scope.is_none_or(|p| !p.only || p.contains(c)))
        .cloned()
        .collect();
    for ContainerGroup {
        project,
        containers,
    } in compose::group_by_project(containers)
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
                Row::ComposeFile(file) => format!("<{}>", file.project),
            })
            .collect()
    }

    #[test]
    fn groups_rows_by_project() {
        let rows = container_rows(&sample(), "", &HashSet::new(), None);
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
            names(&container_rows(&sample(), "", &collapsed, None)),
            ["[web 1/2]", "scratch"]
        );
        // Group counts stay those of the whole project while filtering.
        assert_eq!(
            names(&container_rows(&sample(), "db", &HashSet::new(), None)),
            ["[web 1/2]", "web-db-1"]
        );
        assert_eq!(
            names(&container_rows(&sample(), "WEB", &HashSet::new(), None)),
            ["[web 1/2]", "web-app-1", "web-db-1"]
        );
        assert!(container_rows(&sample(), "nothing", &HashSet::new(), None).is_empty());
    }

    #[test]
    fn matches_any_field() {
        assert!(matches_any("", &[]));
        assert!(matches_any("Gr", &["postgres"]));
        assert!(!matches_any("x", &["postgres", "local"]));
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

    #[test]
    fn records_stats_history_for_sampled_containers_only() {
        let mut state = AppState::default();
        let sample = Stats {
            cpu_percent: 5.0,
            ..Stats::default()
        };
        state.stats = HashMap::from([("a".to_owned(), sample), ("b".to_owned(), sample)]);
        state.record_stats(0.0);
        state.record_stats(2.0);
        assert_eq!(state.history.get("a").map(StatsHistory::len), Some(2));
        // "b" stopped: no sample, so its history goes.
        state.stats.remove("b");
        state.record_stats(4.0);
        assert!(!state.history.contains_key("b"));
        assert_eq!(state.history.get("a").map(StatsHistory::len), Some(3));
        state.clear_engine_data();
        assert!(state.history.is_empty());
    }
}

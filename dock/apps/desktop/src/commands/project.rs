//! The project bar's folder: opening it, its compose files ("Up" from a file
//! without containers) and a compose project's combined panel (logs + ps).

use crate::app::FerrumContainers;
use crate::project::ProjectState;
use crate::state::{DetailState, DetailTab, EngineStatus, Section};
use ferrumdock_engine::compose::{ComposeAction, ComposeProject};
use ferrumdock_engine::compose_ps;
use ferrumdock_engine::project::ComposeFile;
use gpui::Context;
use std::path::PathBuf;

impl FerrumContainers {
    /// Opens `root` as the project: shown in the bar (and recorded in the
    /// recent projects), its compose files read, the list scoped to it.
    pub fn open_project(&mut self, root: PathBuf, cx: &mut Context<Self>) {
        let project = ProjectState::load(root.clone());
        tracing::info!(files = project.files.len(), "project opened");
        self.state.project = Some(project);
        self.project_bar
            .update(cx, |bar, cx| bar.set_project(Some(root), cx));
        if self.state.section != Section::Containers {
            self.select_section(Section::Containers, cx);
        }
        cx.notify();
    }

    /// Re-reads the project's compose files (on refresh).
    pub fn reload_project_files(&mut self) {
        if let Some(project) = self.state.project.as_mut() {
            let only = project.only;
            *project = ProjectState::load(project.root.clone());
            project.only = only;
        }
    }

    pub fn toggle_project_scope(&mut self, cx: &mut Context<Self>) {
        if let Some(project) = self.state.project.as_mut() {
            project.only = !project.only;
            cx.notify();
        }
    }

    /// `docker compose -p <name> --project-directory <root> -f <file> up -d`.
    pub fn compose_up_file(&mut self, file: &ComposeFile, cx: &mut Context<Self>) {
        let Some(root) = self.state.project.as_ref().map(|p| p.root.clone()) else {
            return;
        };
        if let Some(error) = &file.error {
            self.notify(format!("{}: {error}", file.path.display()), true, cx);
            return;
        }
        self.run_compose(file.compose_project(&root), ComposeAction::Up, cx);
    }

    /// Container ids and service names of a compose project, by service.
    fn project_sources(&self, name: &str) -> Vec<(String, String)> {
        let mut sources: Vec<(String, String)> = self
            .state
            .containers
            .iter()
            .filter(|c| c.project() == Some(name))
            .map(|c| (c.id.clone(), c.service().unwrap_or(&c.name).to_owned()))
            .collect();
        sources.sort_by(|a, b| a.1.cmp(&b.1));
        sources
    }

    /// Opens the project panel: combined logs of every service, and `ps`.
    pub fn open_project_panel(
        &mut self,
        project: ComposeProject,
        tab: DetailTab,
        cx: &mut Context<Self>,
    ) {
        let ids = self
            .project_sources(&project.name)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let mut detail = DetailState::for_project(project, ids);
        detail.tab = tab;
        self.state.detail = Some(detail);
        self.detail_active = true;
        self.inputs
            .log_search
            .update(cx, |input, cx| input.set_text("", cx));
        self.start_logs(cx);
        self.load_services(cx);
        cx.notify();
    }

    /// Restarts the combined logs when the project's containers changed
    /// (a service was added or recreated).
    pub(crate) fn sync_project_panel(&mut self, cx: &mut Context<Self>) {
        let Some(name) = self
            .state
            .detail
            .as_ref()
            .and_then(|d| d.project.as_ref())
            .map(|p| p.name.clone())
        else {
            return;
        };
        let ids: Vec<String> = self
            .project_sources(&name)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let Some(detail) = self.state.detail.as_mut() else {
            return;
        };
        let mut known = detail.sources.clone();
        known.sort();
        let mut now = ids.clone();
        now.sort();
        if known != now {
            detail.sources = ids;
            self.start_logs(cx);
        }
    }

    /// `docker compose ps` for the project panel.
    pub fn load_services(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.state.detail.as_ref().and_then(|d| d.project.clone()) else {
            return;
        };
        let EngineStatus::Connected(connection) = &self.status else {
            return;
        };
        let Some(cli) = self.docker_cli.clone() else {
            if let Some(detail) = self.state.detail.as_mut() {
                detail.services = Some(Err(
                    "The docker CLI was not found; it is needed for compose ps.".into(),
                ));
            }
            return;
        };
        let endpoint = connection.candidate.endpoint.clone();
        let name = project.name.clone();
        self.spawn_run(
            cx,
            async move { compose_ps::run_ps(&cli, &endpoint, &project).await },
            move |app, result, _| {
                if let Some(detail) = app
                    .state
                    .detail
                    .as_mut()
                    .filter(|d| d.project.as_ref().is_some_and(|p| p.name == name))
                {
                    detail.services = Some(result);
                }
            },
        );
    }

    /// Log sources for the open panel: `(id, label)`, labels only for projects.
    pub(crate) fn log_sources(&self) -> Vec<(String, String)> {
        match self.state.detail.as_ref() {
            Some(d) if d.project.is_some() => {
                let labels =
                    self.project_sources(d.project.as_ref().map_or("", |p| p.name.as_str()));
                d.sources
                    .iter()
                    .map(|id| {
                        let label = labels
                            .iter()
                            .find(|(i, _)| i == id)
                            .map_or_else(|| id.clone(), |(_, l)| l.clone());
                        (id.clone(), label)
                    })
                    .collect()
            }
            Some(d) => vec![(d.id.clone(), String::new())],
            None => Vec::new(),
        }
    }
}

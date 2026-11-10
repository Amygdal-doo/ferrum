//! Container lifecycle, compose projects, "open shell" and published ports.

use crate::app::FerrumDock;
use crate::state::{Confirm, Dialog, EngineStatus, PendingAction, Section};
use ferrumdock_engine::compose::{self, ComposeAction, ComposeProject};
use ferrumdock_engine::{Container, ContainerAction, DockerCli, ops, shell};
use gpui::Context;
use std::path::PathBuf;

impl FerrumDock {
    pub fn container_action(
        &mut self,
        container: &Container,
        action: ContainerAction,
        cx: &mut Context<Self>,
    ) {
        let id = container.id.clone();
        let name = container.name.clone();
        if !self.state.busy.insert(id.clone()) {
            return;
        }
        let target = id.clone();
        self.with_engine(
            cx,
            move |engine| async move { engine.container_action(&target, action).await },
            move |app, result, cx| {
                app.state.busy.remove(&id);
                app.report(&result, format!("{} {name}", action.past_tense()), cx);
                if result.is_ok() && matches!(action, ContainerAction::Remove { .. }) {
                    app.state.containers.retain(|c| c.id != id);
                    if app.state.detail.as_ref().is_some_and(|d| d.id == id) {
                        app.close_detail(cx);
                    }
                }
                app.refresh(Section::Containers, cx);
            },
        );
        cx.notify();
    }

    /// Asks before removing; running containers are force-removed (said in the dialog).
    pub fn confirm_remove_container(&mut self, container: &Container, cx: &mut Context<Self>) {
        let force = matches!(
            ops::remove_action(container),
            ContainerAction::Remove { force: true }
        );
        let body = if force {
            format!(
                "{} is {}. It will be stopped and removed. Its anonymous volumes are kept.",
                container.name,
                container.state.label()
            )
        } else {
            format!("Remove {}? Its anonymous volumes are kept.", container.name)
        };
        self.ask(
            Confirm {
                title: "Remove container".into(),
                body,
                confirm_label: if force { "Stop and remove" } else { "Remove" }.into(),
                action: PendingAction::RemoveContainer(container.clone()),
            },
            cx,
        );
    }

    pub fn ask(&mut self, confirm: Confirm, cx: &mut Context<Self>) {
        self.state.dialog = Some(Dialog::Confirm(Box::new(confirm)));
        cx.notify();
    }

    pub fn compose(
        &mut self,
        project: ComposeProject,
        action: ComposeAction,
        cx: &mut Context<Self>,
    ) {
        if action == ComposeAction::Down {
            let body = format!(
                "Stops and removes the containers and networks of {}. Volumes and images are kept.",
                project.name
            );
            self.ask(
                Confirm {
                    title: format!("docker compose down ({})", project.name),
                    body,
                    confirm_label: "Down".into(),
                    action: PendingAction::ComposeDown(project),
                },
                cx,
            );
            return;
        }
        self.run_compose(project, action, cx);
    }

    pub fn run_compose(
        &mut self,
        project: ComposeProject,
        action: ComposeAction,
        cx: &mut Context<Self>,
    ) {
        let EngineStatus::Connected(connection) = &self.status else {
            return;
        };
        let Some(cli) = self.docker_cli.clone() else {
            self.notify(
                "The docker CLI was not found; it is needed for compose projects.",
                true,
                cx,
            );
            return;
        };
        let key = format!("compose:{}", project.name);
        if !self.state.busy.insert(key.clone()) {
            return;
        }
        let endpoint = connection.candidate.endpoint.clone();
        let name = project.name.clone();
        self.spawn_run(
            cx,
            async move { compose::run_compose(&cli, &endpoint, &project, action).await },
            move |app, result, cx| {
                app.state.busy.remove(&key);
                app.report(
                    &result,
                    format!("docker compose {} finished for {name}", action.verb()),
                    cx,
                );
                app.refresh(Section::Containers, cx);
            },
        );
        cx.notify();
    }

    /// `docker exec -it <id> sh` in FerrumTerm (if installed) or Terminal.app.
    pub fn open_shell(&mut self, container: &Container, cx: &mut Context<Self>) {
        let EngineStatus::Connected(connection) = &self.status else {
            return;
        };
        let Some(cli) = self.docker_cli.clone() else {
            self.notify(
                "The docker CLI was not found; it is needed to open a shell.",
                true,
                cx,
            );
            return;
        };
        let home = std::env::var_os("HOME").map_or_else(|| PathBuf::from("/"), PathBuf::from);
        let command = shell::exec_command(&cli, &connection.candidate.endpoint, &container.id);
        let ferrumterm = shell::find_ferrumterm(&home, |p| p.exists());
        let plan = shell::plan(&command, &container.name, &home, ferrumterm);
        let target = plan.target_name();
        let name = container.name.clone();
        self.spawn_run(
            cx,
            async move { shell::launch(&plan).await },
            move |app, result, cx| {
                app.report(&result, format!("Opened a shell in {name} ({target})"), cx);
            },
        );
    }

    pub fn open_url(&mut self, url: &str, cx: &mut Context<Self>) {
        cx.open_url(url);
    }

    pub fn toggle_group(&mut self, project: &str, cx: &mut Context<Self>) {
        if !self.state.collapsed.remove(project) {
            self.state.collapsed.insert(project.to_owned());
        }
        cx.notify();
    }
}

/// The CLI is located once at startup (`PATH` + well-known install locations).
pub fn detect_cli() -> Option<DockerCli> {
    let cli = DockerCli::detect();
    match &cli {
        Some(c) => tracing::info!(path = %c.path.display(), "docker CLI"),
        None => tracing::info!("docker CLI not found: compose and shell actions are unavailable"),
    }
    cli
}

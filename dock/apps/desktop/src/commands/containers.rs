//! Container lifecycle, compose projects, "open shell" and published ports.

use crate::app::FerrumContainers;
use crate::state::{Confirm, Dialog, EngineStatus, PendingAction, Section};
use ferrumdock_engine::compose::{ComposeAction, ComposeProject};
use ferrumdock_engine::shell::Shell;
use ferrumdock_engine::{Container, ContainerAction, DockerCli, ops, shell};
use gpui::Context;
use std::path::PathBuf;

impl FerrumContainers {
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

    /// `docker exec -it <id> <shell>` in FerrumTerm (if installed), else
    /// Terminal.app (macOS) or a terminal emulator on `PATH` (Linux). The
    /// shell is the one picked in the detail panel for this container, else auto.
    pub fn open_shell(&mut self, container: &Container, cx: &mut Context<Self>) {
        let choice = self
            .state
            .detail
            .as_ref()
            .filter(|d| d.id == container.id)
            .map_or(Shell::Auto, |d| d.shell);
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
        let command =
            shell::exec_command(&cli, &connection.candidate.endpoint, &container.id, choice);
        let ferrumterm = shell::find_ferrumterm(&home, |p| p.exists());
        let linux = if cfg!(target_os = "linux") {
            shell::find_linux_terminal(std::env::var_os("PATH").as_deref(), |p| p.is_file())
        } else {
            None
        };
        let plan = shell::plan(&command, &container.name, &home, ferrumterm, linux);
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

    /// Asks the selected running container which shells it has (`docker exec … sh -c`).
    pub fn detect_shells(&mut self, cx: &mut Context<Self>) {
        let EngineStatus::Connected(connection) = &self.status else {
            return;
        };
        let (Some(cli), Some(detail)) = (self.docker_cli.clone(), self.state.detail.as_ref())
        else {
            return;
        };
        if detail.project.is_some()
            || !self
                .state
                .container(&detail.id)
                .is_some_and(|c| c.state.is_up())
        {
            return;
        }
        let id = detail.id.clone();
        let endpoint = connection.candidate.endpoint.clone();
        let target = id.clone();
        self.spawn_run(
            cx,
            async move { shell::detect_shells(&cli, &endpoint, &target).await },
            move |app, result, _| {
                if let Some(detail) = app.state.detail.as_mut().filter(|d| d.id == id) {
                    // Unknown (exec failed): keep offering every choice.
                    detail.shells = result.ok();
                }
            },
        );
    }

    pub fn set_shell(&mut self, shell: Shell, cx: &mut Context<Self>) {
        self.update_detail(|d| d.shell = shell, cx);
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

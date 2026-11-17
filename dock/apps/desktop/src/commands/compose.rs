//! Compose commands with visible output, and the Compose tab's editor:
//! open the project's compose file, YAML highlighting while typing, Save,
//! and Validate (`docker compose config`, unsaved text through stdin).

use crate::app::FerrumContainers;
use crate::state::{ComposeRun, EngineStatus, Section};
use ferrum_ui::theme::ActiveTheme;
use ferrumdock_engine::compose::{ComposeAction, ComposeProject};
use ferrumdock_engine::compose_run;
use gpui::{Context, HighlightStyle};
use std::path::Path;

impl FerrumContainers {
    /// The compose project of the open panel.
    fn panel_project(&self) -> Option<ComposeProject> {
        self.state.detail.as_ref()?.project.clone()
    }

    /// Runs `docker compose … <action>`, streaming its output into the
    /// project's [`ComposeRun`] (shown in the Compose tab); a notice says
    /// how it ended.
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
        self.state.compose_runs.insert(
            name.clone(),
            ComposeRun::new(compose_run::command_line(&project, action)),
        );
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();
        let job = self.services.spawn(async move {
            compose_run::run_streaming(&cli, &endpoint, &project, action, |line| {
                let _ = tx.send(line);
            })
            .await
        });
        cx.spawn(async move |this, cx| {
            while let Some(line) = rx.recv().await {
                let mut batch = vec![line];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                let project = name.clone();
                let alive = this.update(cx, |app, cx| {
                    if let Some(run) = app.state.compose_runs.get_mut(&project) {
                        batch.into_iter().for_each(|l| run.push(l));
                    }
                    cx.notify();
                });
                if alive.is_err() {
                    return;
                }
            }
            let result = match job.await {
                Ok(r) => r.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = this.update(cx, |app, cx| {
                app.state.busy.remove(&key);
                let done = format!("docker compose {} finished for {name}", action.verb());
                if let Some(run) = app.state.compose_runs.get_mut(&name) {
                    run.finish(result.clone().map(|()| done.clone()));
                }
                app.report(&result, done, cx);
                app.refresh(Section::Containers, cx);
            });
        })
        .detach();
        cx.notify();
    }

    /// Loads the panel project's compose file into the editor (once).
    pub fn open_compose_file(&mut self, cx: &mut Context<Self>) {
        let Some(project) = self.panel_project() else {
            return;
        };
        if self
            .state
            .detail
            .as_ref()
            .is_some_and(|d| d.compose.saved.is_some() || d.compose.error.is_some())
        {
            return;
        }
        let file = compose_run::compose_file(&project, Path::exists);
        let loaded = match &file {
            None => Err(format!(
                "No compose file is known for {}: its containers have no compose file labels, \
                 or the files were moved.",
                project.name
            )),
            Some(path) => {
                std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
            }
        };
        let text = loaded.as_ref().ok().cloned().unwrap_or_default();
        if let Some(detail) = self.state.detail.as_mut() {
            detail.compose.file = file;
            match loaded {
                Ok(text) => detail.compose.saved = Some(text),
                Err(error) => detail.compose.error = Some(error),
            }
            detail.compose.dirty = false;
        }
        self.inputs
            .compose_editor
            .update(cx, |input, cx| input.set_text(text, cx));
        self.highlight_compose(cx);
        cx.notify();
    }

    /// The editor changed: re-highlight and note unsaved edits.
    pub fn compose_text_changed(&mut self, cx: &mut Context<Self>) {
        let text = self.inputs.compose_editor.read(cx).text().to_owned();
        if let Some(detail) = self.state.detail.as_mut() {
            detail.compose.dirty = detail.compose.saved.as_deref().is_some_and(|s| s != text);
        }
        self.highlight_compose(cx);
        cx.notify();
    }

    fn highlight_compose(&mut self, cx: &mut Context<Self>) {
        let text = self.inputs.compose_editor.read(cx).text().to_owned();
        let syntax = cx.theme().syntax().clone();
        let highlights: Vec<(std::ops::Range<usize>, HighlightStyle)> = crate::yaml::spans(&text)
            .into_iter()
            .filter_map(|(range, name)| Some((range, syntax.style_for_name(name)?)))
            .collect();
        self.inputs
            .compose_editor
            .update(cx, |input, cx| input.set_highlights(highlights, cx));
    }

    /// Writes the editor's text to the compose file.
    pub fn save_compose_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self
            .state
            .detail
            .as_ref()
            .and_then(|d| d.compose.file.clone())
        else {
            return;
        };
        let text = self.inputs.compose_editor.read(cx).text().to_owned();
        let tmp = path.with_extension("ferrum-tmp");
        let result = std::fs::write(&tmp, &text)
            .and_then(|()| std::fs::rename(&tmp, &path))
            .map_err(|e| format!("{}: {e}", path.display()));
        if result.is_ok()
            && let Some(detail) = self.state.detail.as_mut()
        {
            detail.compose.saved = Some(text);
            detail.compose.dirty = false;
        }
        self.report(&result, format!("Saved {}", path.display()), cx);
        self.reload_project_files();
    }

    /// `docker compose config` for the panel's project; unsaved edits are
    /// checked as they are (through stdin), the file is not written.
    pub fn validate_compose(&mut self, cx: &mut Context<Self>) {
        let (Some(project), Some(detail)) = (self.panel_project(), self.state.detail.as_ref())
        else {
            return;
        };
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
        let file = detail.compose.file.clone();
        let unsaved = detail
            .compose
            .dirty
            .then(|| self.inputs.compose_editor.read(cx).text().to_owned());
        let endpoint = connection.candidate.endpoint.clone();
        let name = project.name.clone();
        let command =
            compose_run::config_args(&project, file.as_deref(), unsaved.is_some(), Path::exists);
        let mut run = ComposeRun::new(format!(
            "docker {}{}",
            command.join(" "),
            if unsaved.is_some() {
                "  (unsaved text)"
            } else {
                ""
            }
        ));
        run.push("Validating…".into());
        self.state.compose_runs.insert(name.clone(), run);
        self.spawn_run(
            cx,
            async move {
                compose_run::validate(
                    &cli,
                    &endpoint,
                    &project,
                    file.as_deref(),
                    unsaved.as_deref(),
                )
                .await
            },
            move |app, result, cx| {
                let Some(run) = app.state.compose_runs.get_mut(&name) else {
                    return;
                };
                run.lines.clear();
                let text = match &result {
                    Ok(text) | Err(text) => text.clone(),
                };
                text.lines().for_each(|l| run.push(l.to_owned()));
                run.finish(
                    result
                        .as_ref()
                        .map(|_| "The compose file is valid".to_owned())
                        .map_err(|_| "docker compose config failed".to_owned()),
                );
                let ok = result.is_ok();
                app.notify(
                    if ok {
                        "The compose file is valid"
                    } else {
                        "docker compose config found a problem"
                    },
                    !ok,
                    cx,
                );
            },
        );
        cx.notify();
    }
}

//! The engine picker: `DOCKER_HOST`, Docker contexts (switch, add an
//! `ssh://` engine, remove) and the known sockets.

use crate::app::FerrumContainers;
use crate::state::{Confirm, Dialog, EngineStatus, PendingAction};
use ferrumdock_engine::cli::run_settings;
use ferrumdock_engine::{Candidate, context, endpoint};
use gpui::Context;
use std::convert::Infallible;

impl FerrumContainers {
    /// Opens the picker and lists the engines present on this machine.
    pub fn open_engine_picker(&mut self, cx: &mut Context<Self>) {
        self.state.dialog = Some(Dialog::Engines);
        self.state.engines.error = None;
        self.load_engines(cx);
    }

    /// Re-reads engines and Docker contexts for the picker.
    pub fn load_engines(&mut self, cx: &mut Context<Self>) {
        self.state.engines.loading = true;
        let work = async {
            let contexts = context::detect();
            let mut candidates = endpoint::detect();
            candidates.extend(endpoint::remote_contexts(&contexts));
            Ok::<_, Infallible>((candidates, contexts))
        };
        self.spawn_run(cx, work, |app, result, _| {
            app.state.engines.loading = false;
            if let Ok((candidates, contexts)) = result {
                let picker = &mut app.state.engines;
                picker.candidates = candidates;
                picker.unsupported = contexts
                    .iter()
                    .filter(|c| c.endpoint.is_none())
                    .cloned()
                    .collect();
                picker.contexts = contexts;
            }
        });
        cx.notify();
    }

    /// Connects to `candidate` (and keeps preferring it on reconnects).
    pub fn pick_engine(&mut self, candidate: &Candidate, cx: &mut Context<Self>) {
        self.state.dialog = None;
        self.state.preferred = Some(candidate.clone());
        let current = match &self.status {
            EngineStatus::Connected(c) => Some(c.candidate.endpoint.clone()),
            _ => None,
        };
        if current.as_ref() != Some(&candidate.endpoint) {
            tracing::info!(endpoint = %candidate.endpoint, "engine picked");
            self.connect(false, cx);
        }
        cx.notify();
    }

    /// Runs a `docker context …` command from the picker, then `then` on success.
    fn context_command(
        &mut self,
        args: Vec<String>,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
    ) {
        let Some(cli) = self.docker_cli.clone() else {
            self.state.engines.error =
                Some("The docker CLI was not found; it manages Docker contexts.".into());
            cx.notify();
            return;
        };
        if self.state.engines.busy {
            return;
        }
        self.state.engines.busy = true;
        self.state.engines.error = None;
        tracing::info!(command = %args.join(" "), "docker context");
        self.spawn_run(
            cx,
            async move { run_settings(&cli, &args).await },
            move |app, result, cx| {
                app.state.engines.busy = false;
                match result {
                    Ok(_) => then(app, cx),
                    Err(error) => app.state.engines.error = Some(error),
                }
                app.load_engines(cx);
            },
        );
        cx.notify();
    }

    /// `docker context use <name>`, then connects to it.
    pub fn use_context(&mut self, name: String, cx: &mut Context<Self>) {
        let target = name.clone();
        self.context_command(context::use_args(&name), cx, move |app, cx| {
            app.notify(format!("docker context use {target}"), false, cx);
            app.follow_context(&target, cx);
        });
    }

    /// `docker context create <name> --docker host=ssh://…`, then connects to it.
    pub fn add_ssh_context(&mut self, cx: &mut Context<Self>) {
        let name = self.inputs.context_name.read(cx).text().trim().to_owned();
        let host = self.inputs.context_host.read(cx).text().trim().to_owned();
        let args = match context::create_ssh_args(&name, &host) {
            Ok(args) => args,
            Err(error) => {
                self.state.engines.error = Some(error);
                cx.notify();
                return;
            }
        };
        self.context_command(args, cx, move |app, cx| {
            app.inputs
                .context_name
                .update(cx, |input, cx| input.set_text("", cx));
            app.inputs
                .context_host
                .update(cx, |input, cx| input.set_text("", cx));
            app.notify(format!("Added Docker context {name}"), false, cx);
            app.follow_context(&name, cx);
        });
    }

    /// Asks before `docker context rm`.
    pub fn confirm_remove_context(&mut self, name: String, cx: &mut Context<Self>) {
        self.ask(
            Confirm {
                title: format!("Remove Docker context {name}"),
                body: format!(
                    "Removes the context {name} from the docker CLI. The engine itself is not touched."
                ),
                confirm_label: "Remove".into(),
                action: PendingAction::RemoveContext(name),
            },
            cx,
        );
    }

    pub fn remove_context(&mut self, name: String, cx: &mut Context<Self>) {
        self.state.dialog = Some(Dialog::Engines);
        let target = name.clone();
        self.context_command(context::remove_args(&name), cx, move |app, cx| {
            app.notify(format!("Removed Docker context {target}"), false, cx);
        });
    }

    /// Prefers the context `name` (read back from disk) and reconnects.
    fn follow_context(&mut self, name: &str, cx: &mut Context<Self>) {
        let contexts = context::detect();
        let candidate = contexts.iter().find(|c| c.name == name).and_then(|c| {
            c.endpoint.clone().map(|endpoint| Candidate {
                endpoint,
                source: "Docker context",
                context: Some(c.name.clone()),
            })
        });
        if let Some(candidate) = candidate {
            self.pick_engine(&candidate, cx);
        }
    }
}

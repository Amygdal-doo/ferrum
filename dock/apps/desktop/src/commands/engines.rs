//! The engine picker: `DOCKER_HOST`, Docker contexts and the known sockets.

use crate::app::FerrumDock;
use crate::state::{Dialog, EngineStatus};
use ferrumdock_engine::{Candidate, context, endpoint};
use gpui::Context;
use std::convert::Infallible;

impl FerrumDock {
    /// Opens the picker and lists the engines present on this machine.
    pub fn open_engine_picker(&mut self, cx: &mut Context<Self>) {
        self.state.dialog = Some(Dialog::Engines);
        self.state.engines.loading = true;
        let work = async {
            let contexts = context::detect();
            let unsupported = contexts
                .iter()
                .filter(|c| c.endpoint.is_none())
                .cloned()
                .collect::<Vec<_>>();
            Ok::<_, Infallible>((endpoint::detect(), unsupported))
        };
        self.spawn_run(cx, work, |app, result, _| {
            app.state.engines.loading = false;
            if let Ok((candidates, unsupported)) = result {
                app.state.engines.candidates = candidates;
                app.state.engines.unsupported = unsupported;
            }
        });
        cx.notify();
    }

    /// Connects to `candidate` (and keeps preferring it on reconnects).
    pub fn pick_engine(&mut self, candidate: &Candidate, cx: &mut Context<Self>) {
        self.state.dialog = None;
        self.state.preferred = Some(candidate.endpoint.clone());
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
}

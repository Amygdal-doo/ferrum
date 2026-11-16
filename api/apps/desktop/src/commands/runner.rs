//! Collection runner: plans the run from current state, runs it on the
//! services runtime and streams progress into `AppState::runner`.

use crate::app::FerrumApi;
use crate::state::Dialog;
use crate::state::runner::RunnerState;
use ferrumapi_application::{CancellationToken, RunEvent, plan_run};
use ferrumapi_domain::{CollectionId, ItemId, RequestOrigin};
use gpui::Context;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_RUN: AtomicU64 = AtomicU64::new(1);

impl FerrumApi {
    /// Runs every saved request of `collection` (or of `folder` in it) in
    /// order, replacing any run in progress, and shows the runner dialog.
    pub fn start_run(
        &mut self,
        collection: CollectionId,
        folder: Option<ItemId>,
        cx: &mut Context<Self>,
    ) {
        self.stop_run();
        let state = &self.state;
        let Some(c) = state.collection(collection) else {
            return;
        };
        let title = folder
            .and_then(|id| c.find(id))
            .map_or_else(|| c.name.clone(), |item| item.name().to_owned());
        let items = plan_run(
            collection,
            folder,
            &state.collections,
            &state.globals,
            state.active_environment(),
        );
        if items.is_empty() {
            self.notify(format!("“{title}” has no saved requests to run."), true, cx);
            return;
        }
        let run = NEXT_RUN.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        self.state.runner = Some(RunnerState::new(
            run,
            (collection, folder),
            title,
            &items,
            cancel.clone(),
        ));
        self.state.dialog = Some(Dialog::Runner);

        let (events, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let svc = self.services.clone();
        // Dropping the handle detaches the run; it ends on its own or on Stop.
        drop(
            self.services
                .spawn(async move { svc.run_collection(items, cancel, events).await }),
        );
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.recv().await {
                let update = this.update(cx, |this, cx| {
                    if let RunEvent::Changes(_, changes) = &event {
                        this.apply_script_changes(changes, cx);
                    }
                    if let Some(runner) = this.state.runner.as_mut().filter(|r| r.run == run) {
                        runner.apply(event);
                        cx.notify();
                    }
                });
                if update.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Opens a run's saved request in a tab and closes the runner dialog.
    pub fn open_saved(&mut self, origin: RequestOrigin) {
        let saved = self
            .state
            .collection(origin.collection)
            .and_then(|c| c.find_request(origin.item).cloned());
        if let Some(saved) = saved {
            self.open_request(saved.name, saved.request, Some(origin));
            self.state.dialog = None;
        }
    }

    /// Stops the run in progress (no-op when none is running).
    pub fn stop_run(&mut self) {
        if let Some(runner) = self.state.runner.as_ref().filter(|r| r.running) {
            runner.stop();
        }
    }
}

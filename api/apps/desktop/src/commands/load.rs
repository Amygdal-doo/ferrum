//! Load test of a tab's request: prepared like Send (auth inheritance,
//! variables), run on the services runtime, progress streamed into state.

use crate::app::FerrumApi;
use crate::state::Dialog;
use crate::state::tools::LoadTestState;
use ferrumapi_application::{CancellationToken, load_target_warning};
use ferrumapi_domain::TabId;
use gpui::Context;
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_LOAD_RUN: AtomicU64 = AtomicU64::new(1);

impl FerrumApi {
    /// Shows the load-test dialog for `tab_id`. A run in progress (for any
    /// tab) keeps its dialog; otherwise the form starts from the last values.
    pub fn open_load_test(&mut self, tab_id: TabId) {
        let Some(tab) = self.state.tab(tab_id) else {
            return;
        };
        let running = self.state.load_test.as_ref().is_some_and(|l| l.running);
        if !running {
            let prepared = self.state.prepare_tab(tab);
            let warning = load_target_warning(&prepared);
            let title = format!("{} {}", tab.request.method, tab.request.label());
            let mut next = LoadTestState::new(tab_id, title, warning);
            if let Some(last) = &self.state.load_test {
                next.total_text.clone_from(&last.total_text);
                next.concurrency_text.clone_from(&last.concurrency_text);
                next.duration_text.clone_from(&last.duration_text);
            }
            self.state.load_test = Some(next);
        }
        self.state.dialog = Some(Dialog::LoadTest);
    }

    pub fn start_load_test(&mut self, cx: &mut Context<Self>) {
        let Some(tab_id) = self
            .state
            .load_test
            .as_ref()
            .filter(|l| !l.running)
            .map(|l| l.tab_id)
        else {
            return;
        };
        let Some(prepared) = self.state.tab(tab_id).map(|t| self.state.prepare_tab(t)) else {
            return;
        };
        let warning = load_target_warning(&prepared);
        let run = NEXT_LOAD_RUN.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        let Some(load) = self.state.load_test.as_mut() else {
            return;
        };
        load.warning = warning;
        let config = match load.begin(run, cancel.clone()) {
            Ok(config) => config,
            Err(e) => {
                load.error = Some(e);
                return;
            }
        };

        let (events, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let svc = self.services.clone();
        drop(self.services.spawn(async move {
            svc.run_load_test(prepared, config, cancel, events).await;
        }));
        cx.spawn(async move |this, cx| {
            while let Some(event) = rx.recv().await {
                let update = this.update(cx, |this, cx| {
                    if let Some(load) = this.state.load_test.as_mut().filter(|l| l.run == run) {
                        load.apply(event);
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

    pub fn stop_load_test(&mut self) {
        if let Some(load) = &self.state.load_test {
            load.stop();
        }
    }
}

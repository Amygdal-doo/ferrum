//! The container detail panel: live log streaming and inspect.

use crate::app::FerrumDock;
use crate::state::{DetailState, DetailTab, LOG_CAPACITY};
use ferrumdock_engine::logs::LogBuffer;
use ferrumdock_engine::{LogChunk, LogOptions};
use futures::StreamExt;
use gpui::{Context, ScrollStrategy};
use std::time::Duration;

/// Lines fetched when the panel opens (then followed live).
const INITIAL_TAIL: u32 = 2_000;
/// Coalesces log updates into at most ~20 renders per second.
const BATCH_DELAY: Duration = Duration::from_millis(50);

impl FerrumDock {
    pub fn select_container(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.state.detail.as_ref().is_some_and(|d| d.id == id) {
            return;
        }
        self.state.detail = Some(DetailState::new(id.to_owned()));
        self.inputs
            .log_search
            .update(cx, |input, cx| input.set_text("", cx));
        self.start_logs(cx);
        cx.notify();
    }

    pub fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.stop_logs();
        self.state.detail = None;
        cx.notify();
    }

    pub fn stop_logs(&mut self) {
        if let Some((_task, abort)) = self.log_task.take() {
            abort.abort();
        }
    }

    /// (Re)starts following the selected container's logs from the last lines.
    pub fn start_logs(&mut self, cx: &mut Context<Self>) {
        self.stop_logs();
        let Some(engine) = self.engine() else { return };
        let Some(detail) = self.state.detail.as_mut() else {
            return;
        };
        detail.logs = LogBuffer::new(LOG_CAPACITY, true);
        detail.streaming = true;
        detail.log_error = None;
        detail.refilter();
        let id = detail.id.clone();

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<LogChunk, String>>();
        let target = id.clone();
        let producer = self.services.spawn(async move {
            let options = LogOptions {
                follow: true,
                tail: Some(INITIAL_TAIL),
                timestamps: true,
            };
            let mut stream = engine.logs(&target, options);
            while let Some(item) = stream.next().await {
                if tx.send(item.map_err(|e| e.to_string())).is_err() {
                    break;
                }
            }
        });
        let abort = producer.abort_handle();
        let consumer = cx.spawn(async move |this, cx| {
            while let Some(first) = rx.recv().await {
                let mut batch = vec![first];
                while let Ok(more) = rx.try_recv() {
                    batch.push(more);
                }
                if this
                    .update(cx, |app, cx| app.apply_logs(&id, batch, cx))
                    .is_err()
                {
                    return;
                }
                cx.background_executor().timer(BATCH_DELAY).await;
            }
            let _ = this.update(cx, |app, cx| {
                if let Some(detail) = app.state.detail.as_mut().filter(|d| d.id == id) {
                    detail.logs.finish();
                    detail.streaming = false;
                    detail.refilter();
                    cx.notify();
                }
            });
        });
        self.log_task = Some((consumer, abort));
    }

    fn apply_logs(
        &mut self,
        id: &str,
        batch: Vec<Result<LogChunk, String>>,
        cx: &mut Context<Self>,
    ) {
        let Some(detail) = self.state.detail.as_mut().filter(|d| d.id == id) else {
            return;
        };
        for item in batch {
            match item {
                Ok(chunk) => detail.logs.push(&chunk),
                Err(error) => detail.log_error = Some(error),
            }
        }
        detail.refilter();
        if detail.follow {
            let count = detail.visible_count();
            if count > 0 {
                self.log_scroll
                    .scroll_to_item(count - 1, ScrollStrategy::Bottom);
            }
        }
        cx.notify();
    }

    /// After a containers refresh: close the panel if its container is gone,
    /// resume logs when a stopped container is running again.
    pub fn sync_detail(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = &self.state.detail else {
            return;
        };
        match self.state.container(&detail.id) {
            None => self.close_detail(cx),
            Some(c) if c.state.is_up() && !detail.streaming && detail.log_error.is_none() => {
                self.start_logs(cx);
            }
            Some(_) => {}
        }
    }

    pub fn set_detail_tab(&mut self, tab: DetailTab, cx: &mut Context<Self>) {
        let Some(detail) = self.state.detail.as_mut() else {
            return;
        };
        detail.tab = tab;
        if tab == DetailTab::Inspect && detail.inspect.is_none() {
            self.load_inspect(cx);
        }
        cx.notify();
    }

    pub fn load_inspect(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.state.detail.as_ref().map(|d| d.id.clone()) else {
            return;
        };
        let target = id.clone();
        self.with_engine(
            cx,
            move |e| async move { e.inspect(&target).await },
            move |app, result, _| {
                if let Some(detail) = app.state.detail.as_mut().filter(|d| d.id == id) {
                    detail.inspect = Some(result);
                }
            },
        );
    }

    pub fn set_log_search(&mut self, text: String, cx: &mut Context<Self>) {
        if let Some(detail) = self.state.detail.as_mut() {
            detail.search = text;
            detail.refilter();
            cx.notify();
        }
    }

    pub fn toggle_follow(&mut self, cx: &mut Context<Self>) {
        if let Some(detail) = self.state.detail.as_mut() {
            detail.follow = !detail.follow;
            let count = detail.visible_count();
            if detail.follow && count > 0 {
                self.log_scroll
                    .scroll_to_item(count - 1, ScrollStrategy::Bottom);
            }
            cx.notify();
        }
    }

    pub fn update_detail(&mut self, f: impl FnOnce(&mut DetailState), cx: &mut Context<Self>) {
        if let Some(detail) = self.state.detail.as_mut() {
            f(detail);
            cx.notify();
        }
    }
}

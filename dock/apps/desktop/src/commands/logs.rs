//! The detail panel: live log streaming (one container or a whole compose
//! project), the since / regex / save controls, and inspect.

use crate::app::FerrumContainers;
use crate::state::{DetailState, DetailTab, LOG_CAPACITY};
use ferrumdock_engine::logs::{LogBuffer, Since, export_file_name};
use ferrumdock_engine::{LogChunk, LogOptions, logmux};
use futures::StreamExt;
use gpui::{Context, ScrollStrategy};
use std::time::Duration;

/// Lines fetched when the panel opens (then followed live).
const INITIAL_TAIL: u32 = 2_000;
/// Coalesces log updates into at most ~20 renders per second.
const BATCH_DELAY: Duration = Duration::from_millis(50);

impl FerrumContainers {
    pub fn select_container(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.state.detail.as_ref().is_some_and(|d| d.id == id) {
            return;
        }
        self.state.detail = Some(DetailState::new(id.to_owned()));
        self.detail_active = true;
        self.inputs
            .log_search
            .update(cx, |input, cx| input.set_text("", cx));
        self.start_logs(cx);
        self.detect_shells(cx);
        cx.notify();
    }

    pub fn close_detail(&mut self, cx: &mut Context<Self>) {
        self.stop_logs();
        self.state.detail = None;
        self.detail_active = false;
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
        let project = detail.project.is_some();
        let options = LogOptions {
            follow: true,
            tail: Some(INITIAL_TAIL),
            timestamps: true,
            since: detail.since.since(chrono::Utc::now().timestamp()),
        };
        let sources = self.log_sources();
        if project && sources.is_empty() {
            if let Some(detail) = self.state.detail.as_mut() {
                detail.streaming = false;
                detail.log_error = Some("The project has no containers yet.".into());
            }
            return;
        }

        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<Result<LogChunk, String>>();
        let producer = self.services.spawn(async move {
            let mut stream = if project {
                logmux::project_logs(&engine, &sources, options)
            } else {
                let target = sources
                    .first()
                    .map(|(id, _)| id.clone())
                    .unwrap_or_default();
                engine.logs(&target, options)
            };
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
        if detail.project.is_some() {
            self.sync_project_panel(cx);
            return;
        }
        match self.state.container(&detail.id) {
            None if detail.grace > 0 => {
                if let Some(detail) = self.state.detail.as_mut() {
                    detail.grace -= 1;
                }
            }
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
        let list_files = (tab == DetailTab::Files && detail.files.listing.is_none())
            .then(|| detail.files.path.clone());
        if tab == DetailTab::Inspect && detail.inspect.is_none() {
            self.load_inspect(cx);
        }
        if tab == DetailTab::Services {
            self.load_services(cx);
        }
        if let Some(path) = list_files {
            self.load_dir(path, cx);
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

    /// Re-requests the logs from the chosen point in time.
    pub fn set_since(&mut self, since: Since, cx: &mut Context<Self>) {
        let Some(detail) = self.state.detail.as_mut() else {
            return;
        };
        if detail.since != since {
            detail.since = since;
            self.start_logs(cx);
        }
        cx.notify();
    }

    pub fn toggle_regex(&mut self, cx: &mut Context<Self>) {
        self.update_detail(
            |d| {
                d.regex = !d.regex;
                d.refilter();
            },
            cx,
        );
    }

    /// "Save…": the visible lines (search applied), plain text, via the save dialog.
    pub fn save_logs(&mut self, cx: &mut Context<Self>) {
        let Some(detail) = self.state.detail.as_ref() else {
            return;
        };
        let text = detail
            .logs
            .export(detail.matches.as_deref(), detail.timestamps);
        let name = match &detail.project {
            Some(p) => p.name.clone(),
            None => self
                .state
                .container(&detail.id)
                .map_or_else(|| detail.id.clone(), |c| c.name.clone()),
        };
        let file_name = export_file_name(&name, chrono::Local::now());
        let dir = std::env::var_os("HOME")
            .map(|h| std::path::PathBuf::from(h).join("Downloads"))
            .filter(|d| d.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let receiver = cx.prompt_for_new_path(&dir, Some(&file_name));
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(path))) = receiver.await else {
                return;
            };
            let lines = text.lines().count();
            let result = std::fs::write(&path, text).map_err(|e| e.to_string());
            let _ = this.update(cx, |app, cx| {
                app.report(
                    &result,
                    format!("Saved {lines} lines to {}", path.display()),
                    cx,
                );
            });
        })
        .detach();
    }

    pub fn update_detail(&mut self, f: impl FnOnce(&mut DetailState), cx: &mut Context<Self>) {
        if let Some(detail) = self.state.detail.as_mut() {
            f(detail);
            cx.notify();
        }
    }
}

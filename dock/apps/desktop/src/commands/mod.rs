//! UI commands, as methods on the root view. Engine I/O runs on the services
//! runtime; results are applied on the GPUI foreground thread.

pub mod connection;
pub mod containers;
pub mod engines;
pub mod logs;
pub mod project;
pub mod resources;
pub mod run;
pub mod vm;

use crate::app::FerrumContainers;
use crate::state::{EngineStatus, Notice};
use ferrumdock_engine::{EngineError, SharedEngine};
use gpui::Context;
use std::future::Future;
use std::time::Duration;

impl FerrumContainers {
    pub fn engine(&self) -> Option<SharedEngine> {
        match &self.status {
            EngineStatus::Connected(c) => Some(c.engine.clone()),
            _ => None,
        }
    }

    /// Runs `work(engine)` on the runtime, then `then` on the view with its result.
    /// Does nothing when no engine is connected.
    pub fn with_engine<F, Fut, T>(
        &self,
        cx: &mut Context<Self>,
        work: F,
        then: impl FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) + 'static,
    ) where
        F: FnOnce(SharedEngine) -> Fut,
        Fut: Future<Output = Result<T, EngineError>> + Send + 'static,
        T: Send + 'static,
    {
        let Some(engine) = self.engine() else { return };
        self.spawn_run(cx, work(engine), then);
    }

    /// Runs `future` on the runtime, then `then` on the view.
    pub fn spawn_run<Fut, T, E>(
        &self,
        cx: &mut Context<Self>,
        future: Fut,
        then: impl FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) + 'static,
    ) where
        Fut: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        let handle = self.services.spawn(future);
        cx.spawn(async move |this, cx| {
            let result = match handle.await {
                Ok(inner) => inner.map_err(|e| e.to_string()),
                Err(e) => Err(format!("background task failed: {e}")),
            };
            if let Err(error) = this.update(cx, |app, cx| {
                then(app, result, cx);
                cx.notify();
            }) {
                tracing::debug!(%error, "view closed before a background task finished");
            }
        })
        .detach();
    }

    /// Shows a transient notice; errors stay longer.
    pub fn notify(&mut self, message: impl Into<String>, error: bool, cx: &mut Context<Self>) {
        let notice = Notice {
            message: message.into(),
            error,
        };
        self.state.notice = Some(notice.clone());
        let secs = if error { 10 } else { 4 };
        self.notice_task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_secs(secs))
                .await;
            let _ = this.update(cx, |app, cx| {
                if app.state.notice.as_ref() == Some(&notice) {
                    app.state.notice = None;
                    cx.notify();
                }
            });
        }));
        cx.notify();
    }

    /// Reports `result` as a notice: `ok` on success, the error otherwise.
    pub fn report<T>(
        &mut self,
        result: &Result<T, String>,
        ok: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(_) => self.notify(ok, false, cx),
            Err(error) => self.notify(error.clone(), true, cx),
        }
    }
}

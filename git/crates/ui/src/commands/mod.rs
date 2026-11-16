//! UI commands, as methods on the root view. Each one hands I/O to the
//! services runtime via `AppServices::spawn` and only touches UI state on the
//! GPUI foreground thread.

pub mod conflicts;
pub mod history;
pub mod host;
pub mod profiles;
pub mod pulls;
pub mod repo;
pub mod workspace;

use crate::app::GitView;
use crate::state::Notice;
use ferrumgit_application::AppServices;
use gpui::{ClipboardItem, Context, Task};
use std::future::Future;
use std::time::Duration;

/// Runs `future` on the services runtime and awaits it from the UI task.
pub async fn background<F, T>(services: &AppServices, future: F) -> Result<T, String>
where
    F: Future<Output = T> + Send + 'static,
    T: Send + 'static,
{
    services
        .spawn(future)
        .await
        .map_err(|e| format!("background task failed: {e}"))
}

/// Flattens a background result whose inner error is displayable.
pub async fn run<F, T, E>(services: &AppServices, future: F) -> Result<T, String>
where
    F: Future<Output = Result<T, E>> + Send + 'static,
    T: Send + 'static,
    E: std::fmt::Display + Send + 'static,
{
    background(services, future)
        .await
        .and_then(|r| r.map_err(|e| e.to_string()))
}

impl GitView {
    /// Runs `future` on the services runtime, then `then` on the view with its result.
    pub fn spawn_run<F, T, E>(
        &self,
        cx: &mut Context<Self>,
        future: F,
        then: impl FnOnce(&mut Self, Result<T, String>, &mut Context<Self>) + 'static,
    ) where
        F: Future<Output = Result<T, E>> + Send + 'static,
        T: Send + 'static,
        E: std::fmt::Display + Send + 'static,
    {
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let result = run(&services, future).await;
            if let Err(error) = this.update(cx, |app, cx| {
                then(app, result, cx);
                cx.notify();
            }) {
                tracing::debug!(error = %error, "view closed before a background task finished");
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
            if let Err(error) = this.update(cx, |app, cx| {
                if app.state.notice.as_ref() == Some(&notice) {
                    app.state.notice = None;
                    cx.notify();
                }
            }) {
                tracing::debug!(error = %error, "view closed before a notice expired");
            }
        }));
        cx.notify();
    }

    pub fn copy(&mut self, text: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text.to_owned()));
    }

    /// Native folder / file picker; `then` gets the chosen path.
    pub fn pick_path(
        &self,
        cx: &mut Context<Self>,
        title: &'static str,
        directories: bool,
        then: impl FnOnce(&mut Self, std::path::PathBuf, &mut Context<Self>) + 'static,
    ) {
        let receiver = cx.prompt_for_paths(gpui::PathPromptOptions {
            files: !directories,
            directories,
            multiple: false,
            prompt: Some(title.into()),
        });
        let task: Task<()> = cx.spawn(async move |this, cx| {
            let path = match receiver.await {
                Ok(Ok(Some(paths))) => paths.into_iter().next(),
                Ok(Ok(None)) | Err(_) => None,
                Ok(Err(error)) => {
                    tracing::warn!(error = %error, "file dialog failed");
                    None
                }
            };
            let Some(path) = path else { return };
            if let Err(error) = this.update(cx, |app, cx| {
                then(app, path, cx);
                cx.notify();
            }) {
                tracing::debug!(error = %error, "view closed while a file dialog was open");
            }
        });
        task.detach();
    }
}

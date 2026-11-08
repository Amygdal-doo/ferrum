//! File dialogs (UI thread) plus import / export / save (services runtime).

use crate::app::FerrumApi;
use ferrumapi_domain::{Collection, HttpResponse};
use gpui::{ClipboardItem, Context};
use std::path::PathBuf;

/// Native "open file" dialog.
pub async fn pick_file(title: &str, json_only: bool) -> Option<PathBuf> {
    let mut dialog = rfd::AsyncFileDialog::new().set_title(title);
    if json_only {
        dialog = dialog.add_filter("JSON", &["json"]);
    }
    dialog.pick_file().await.map(|h| h.path().to_path_buf())
}

/// Native "save file" dialog.
pub async fn pick_save(title: &str, file_name: &str) -> Option<PathBuf> {
    rfd::AsyncFileDialog::new()
        .set_title(title)
        .set_file_name(file_name)
        .save_file()
        .await
        .map(|h| h.path().to_path_buf())
}

/// File name for display, never the full path.
#[must_use]
pub fn file_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map_or_else(|| path.to_owned(), |n| n.to_string_lossy().into_owned())
}

/// Suggested file name from the response content type.
#[must_use]
pub fn response_file_name(resp: &HttpResponse) -> &'static str {
    let ct = resp.content_type().unwrap_or_default();
    if resp.is_json() {
        "response.json"
    } else if resp.is_html() {
        "response.html"
    } else if ct.contains("xml") {
        "response.xml"
    } else if ct.starts_with("text/") {
        "response.txt"
    } else {
        "response.bin"
    }
}

impl FerrumApi {
    /// Shows a native picker, then runs `then` on the UI thread with the path.
    pub fn pick_then(
        &self,
        cx: &mut Context<Self>,
        title: &'static str,
        json_only: bool,
        then: impl FnOnce(&mut Self, PathBuf, &mut Context<Self>) + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let Some(path) = pick_file(title, json_only).await else {
                return;
            };
            if let Err(error) = this.update(cx, |this, cx| {
                then(this, path, cx);
                cx.notify();
            }) {
                tracing::warn!(%error, "window closed while a file dialog was open");
            }
        })
        .detach();
    }

    pub fn import_collection(&mut self, cx: &mut Context<Self>) {
        self.pick_then(cx, "Import Postman collection", true, |this, path, cx| {
            let svc = this.services.clone();
            this.io(
                cx,
                async move { svc.import_postman_file(path).await },
                |this, result, cx| match result {
                    Ok(c) => {
                        let name = c.name.clone();
                        this.state.expanded.insert(c.id.0);
                        this.state.collections.push(c);
                        this.notify(format!("Imported “{name}”."), false, cx);
                    }
                    Err(e) => this.notify(format!("Import failed: {e}"), true, cx),
                },
            );
        });
    }

    pub fn import_environment(&mut self, cx: &mut Context<Self>) {
        self.pick_then(cx, "Import Postman environment", true, |this, path, cx| {
            let svc = this.services.clone();
            this.io(
                cx,
                async move { svc.import_environment_file(path).await },
                |this, result, cx| match result {
                    Ok(env) => {
                        let name = env.name.clone();
                        this.state.environments.push(env);
                        this.notify(format!("Imported environment “{name}”."), false, cx);
                    }
                    Err(e) => this.notify(format!("Import failed: {e}"), true, cx),
                },
            );
        });
    }

    fn save_as_then(
        &self,
        cx: &mut Context<Self>,
        title: &'static str,
        suggested: String,
        then: impl FnOnce(&mut Self, PathBuf, &mut Context<Self>) + 'static,
    ) {
        cx.spawn(async move |this, cx| {
            let Some(path) = pick_save(title, &suggested).await else {
                return;
            };
            if let Err(error) = this.update(cx, |this, cx| {
                then(this, path, cx);
                cx.notify();
            }) {
                tracing::warn!(%error, "window closed while a file dialog was open");
            }
        })
        .detach();
    }

    pub fn export_collection(&mut self, c: Collection, cx: &mut Context<Self>) {
        let file = format!(
            "{}.postman_collection.json",
            c.name.replace(['/', '\\'], "-")
        );
        self.save_as_then(cx, "Export collection", file, move |this, path, cx| {
            let svc = this.services.clone();
            this.io(
                cx,
                async move { svc.export_postman_file(c, path).await },
                |this, result, cx| match result {
                    Ok(()) => this.notify("Collection exported.", false, cx),
                    Err(e) => this.notify(format!("Export failed: {e}"), true, cx),
                },
            );
        });
    }

    pub fn save_response(&mut self, bytes: Vec<u8>, name: &'static str, cx: &mut Context<Self>) {
        self.save_as_then(
            cx,
            "Save response",
            name.to_owned(),
            move |this, path, cx| {
                let svc = this.services.clone();
                this.io(
                    cx,
                    async move { svc.save_bytes(bytes, path).await },
                    |this, result, cx| match result {
                        Ok(()) => this.notify("Response saved.", false, cx),
                        Err(e) => this.notify(format!("Could not save: {e}"), true, cx),
                    },
                );
            },
        );
    }

    /// Writes an HTML body to a temp file and opens it in the default browser
    /// (the webview's sandboxed inline preview has no native equivalent).
    pub fn open_html_preview(&mut self, html: String, cx: &mut Context<Self>) {
        let path =
            std::env::temp_dir().join(format!("ferrumapi-preview-{}.html", uuid::Uuid::new_v4()));
        let target = path.clone();
        self.io(
            cx,
            async move { tokio::fs::write(&target, html).await },
            move |this, result, cx| match result {
                Ok(()) => cx.open_url(&format!("file://{}", path.display())),
                Err(e) => this.notify(format!("Could not open preview: {e}"), true, cx),
            },
        );
    }

    pub fn copy_text(&mut self, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        self.notify("Copied to clipboard.", false, cx);
    }
}

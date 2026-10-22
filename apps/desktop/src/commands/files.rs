//! File dialogs (UI thread) plus import / export / save (services runtime).

use super::run;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::{Collection, HttpResponse};
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

pub fn import_collection(mut state: AppState, services: AppServices) {
    spawn(async move {
        let Some(path) = pick_file("Import Postman collection", true).await else {
            return;
        };
        let svc = services.clone();
        match run(
            &services,
            async move { svc.import_postman_file(path).await },
        )
        .await
        {
            Ok(c) => {
                let name = c.name.clone();
                state.expanded.write().insert(c.id.0);
                state.collections.write().push(c);
                state.notify(format!("Imported “{name}”."), false);
            }
            Err(e) => state.notify(format!("Import failed: {e}"), true),
        }
    });
}

pub fn import_environment(mut state: AppState, services: AppServices) {
    spawn(async move {
        let Some(path) = pick_file("Import Postman environment", true).await else {
            return;
        };
        let svc = services.clone();
        match run(
            &services,
            async move { svc.import_environment_file(path).await },
        )
        .await
        {
            Ok(env) => {
                let name = env.name.clone();
                state.environments.write().push(env);
                state.notify(format!("Imported environment “{name}”."), false);
            }
            Err(e) => state.notify(format!("Import failed: {e}"), true),
        }
    });
}

pub fn export_collection(state: AppState, services: AppServices, c: Collection) {
    spawn(async move {
        let file = format!(
            "{}.postman_collection.json",
            c.name.replace(['/', '\\'], "-")
        );
        let Some(path) = pick_save("Export collection", &file).await else {
            return;
        };
        let svc = services.clone();
        match run(
            &services,
            async move { svc.export_postman_file(c, path).await },
        )
        .await
        {
            Ok(()) => state.notify("Collection exported.", false),
            Err(e) => state.notify(format!("Export failed: {e}"), true),
        }
    });
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

pub fn save_response(state: AppState, services: AppServices, bytes: Vec<u8>, name: &'static str) {
    spawn(async move {
        let Some(path) = pick_save("Save response", name).await else {
            return;
        };
        let svc = services.clone();
        match run(&services, async move { svc.save_bytes(bytes, path).await }).await {
            Ok(()) => state.notify("Response saved.", false),
            Err(e) => state.notify(format!("Could not save: {e}"), true),
        }
    });
}

/// Copies `text` via the webview clipboard API (with a fallback).
pub fn copy_text(state: AppState, text: String) {
    let literal = serde_json::to_string(&text).unwrap_or_else(|_| "\"\"".into());
    let js = format!(
        "const t = {literal};\
         try {{ await navigator.clipboard.writeText(t); return true; }} catch (_) {{\
           const a = document.createElement('textarea'); a.value = t; document.body.appendChild(a);\
           a.select(); const ok = document.execCommand('copy'); a.remove(); return ok; }}"
    );
    spawn(async move {
        match document::eval(&js).join::<bool>().await {
            Ok(true) => state.notify("Copied to clipboard.", false),
            _ => state.notify("Could not copy to clipboard.", true),
        }
    });
}

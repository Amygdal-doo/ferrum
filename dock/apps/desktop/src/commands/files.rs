//! The Files tab: browsing a container's file system, viewing text files,
//! downloading a file to this machine and uploading one (after a confirm).
//! See `ferrumdock_engine::files` for how each is done.

use crate::app::FerrumContainers;
use crate::state::{Confirm, DetailTab, PendingAction, Section};
use ferrumdock_engine::archive::EntryKind;
use ferrumdock_engine::{files, format};
use gpui::{Context, PathPromptOptions};
use std::path::PathBuf;

impl FerrumContainers {
    /// The container of the open detail panel (not a compose project).
    fn files_container(&self) -> Option<String> {
        self.state
            .detail
            .as_ref()
            .filter(|d| d.project.is_none())
            .map(|d| d.id.clone())
    }

    /// Lists the folder `path` in the Files tab.
    pub fn load_dir(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(id) = self.files_container() else {
            return;
        };
        let path = files::normalize(&path);
        if let Some(detail) = self.state.detail.as_mut() {
            detail.files.loading = true;
            detail.files.path.clone_from(&path);
        }
        self.inputs
            .files_path
            .update(cx, |input, cx| input.set_text(path.clone(), cx));
        let target = id.clone();
        self.with_engine(
            cx,
            move |engine| async move { files::list_dir(&engine, &target, &path).await },
            move |app, result, _| {
                if let Some(detail) = app.state.detail.as_mut().filter(|d| d.id == id) {
                    detail.files.loading = false;
                    detail.files.listing = Some(result);
                }
            },
        );
        cx.notify();
    }

    /// The path typed in the Files tab's path field.
    pub fn submit_files_path(&mut self, cx: &mut Context<Self>) {
        let path = self.inputs.files_path.read(cx).text().trim().to_owned();
        self.load_dir(path, cx);
    }

    pub fn files_up(&mut self, cx: &mut Context<Self>) {
        if let Some(path) = self.state.detail.as_ref().map(|d| d.files.path.clone()) {
            self.load_dir(files::parent(&path), cx);
        }
    }

    /// A row was clicked: folders (and links, which may point at one) open,
    /// files go to the viewer.
    pub fn open_file_entry(&mut self, name: &str, cx: &mut Context<Self>) {
        let Some(files) = self.state.detail.as_ref().map(|d| &d.files) else {
            return;
        };
        let Some(entry) = files.entry(name) else {
            return;
        };
        let path = files::join(&files.path, name);
        match entry.kind {
            EntryKind::Dir => self.load_dir(path, cx),
            EntryKind::Symlink => self.open_link(path, cx),
            EntryKind::File | EntryKind::Other => self.preview_file(path, cx),
        }
    }

    /// Follows a link: lists it when it is a folder, else views it.
    fn open_link(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(id) = self.files_container() else {
            return;
        };
        let target = id.clone();
        let dir = path.clone();
        self.with_engine(
            cx,
            move |engine| async move { files::list_dir(&engine, &target, &dir).await },
            move |app, result, cx| match result {
                Ok(listing) => {
                    if let Some(detail) = app.state.detail.as_mut().filter(|d| d.id == id) {
                        detail.files.path.clone_from(&listing.path);
                        detail.files.listing = Some(Ok(listing));
                    }
                    app.inputs
                        .files_path
                        .update(cx, |input, cx| input.set_text(path, cx));
                }
                Err(_) => app.preview_file(path, cx),
            },
        );
    }

    /// Reads the first part of `path` into the viewer.
    pub fn preview_file(&mut self, path: String, cx: &mut Context<Self>) {
        let Some(id) = self.files_container() else {
            return;
        };
        if let Some(detail) = self.state.detail.as_mut() {
            detail.files.preview = Some((path.clone(), None));
        }
        self.inputs
            .file_view
            .update(cx, |input, cx| input.set_text("", cx));
        let target = id.clone();
        let file = path.clone();
        self.with_engine(
            cx,
            move |engine| async move { files::read_file(&engine, &target, &file).await },
            move |app, result, cx| {
                let text = result
                    .as_ref()
                    .ok()
                    .and_then(|c| c.text())
                    .map(str::to_owned);
                if let Some(detail) = app.state.detail.as_mut().filter(|d| d.id == id)
                    && detail
                        .files
                        .preview
                        .as_ref()
                        .is_some_and(|(p, _)| *p == path)
                {
                    detail.files.preview = Some((path, Some(result)));
                    if let Some(text) = text {
                        app.inputs
                            .file_view
                            .update(cx, |input, cx| input.set_text(text, cx));
                    }
                }
            },
        );
        cx.notify();
    }

    pub fn close_preview(&mut self, cx: &mut Context<Self>) {
        self.update_detail(|d| d.files.preview = None, cx);
    }

    /// Saves the container's file `path` where the save dialog says
    /// (`~/Downloads/<name>` by default).
    pub fn download_file(&mut self, path: String, cx: &mut Context<Self>) {
        let (Some(id), Some(engine)) = (self.files_container(), self.engine()) else {
            return;
        };
        let name = files::base_name(&path);
        let dir = std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Downloads"))
            .filter(|d| d.is_dir())
            .unwrap_or_else(std::env::temp_dir);
        let receiver = cx.prompt_for_new_path(&dir, Some(&name));
        let services = self.services.clone();
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(dest))) = receiver.await else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                app.set_transfer(Some(format!("Downloading {name}…")), cx);
            });
            let target = dest.clone();
            let job = services
                .spawn(async move { files::download_file(&engine, &id, &path, &target).await });
            let result = match job.await {
                Ok(r) => r.map_err(|e| e.to_string()),
                Err(e) => Err(e.to_string()),
            };
            let _ = this.update(cx, |app, cx| {
                app.set_transfer(None, cx);
                let message = result
                    .as_ref()
                    .map(|size| {
                        format!(
                            "Saved {} ({}) to {}",
                            name,
                            format::bytes(*size),
                            dest.display()
                        )
                    })
                    .unwrap_or_default();
                app.report(&result, message, cx);
            });
        })
        .detach();
    }

    /// Picks a local file, then asks before copying it into the folder shown.
    pub fn choose_upload(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.files_container() else {
            return;
        };
        let Some(dir) = self.state.detail.as_ref().map(|d| d.files.path.clone()) else {
            return;
        };
        let name = self
            .state
            .container(&id)
            .map_or_else(|| id.clone(), |c| c.name.clone());
        let receiver = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Upload".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = receiver.await else {
                return;
            };
            let Some(local) = paths.into_iter().next() else {
                return;
            };
            let size = std::fs::metadata(&local).map(|m| m.len()).unwrap_or(0);
            let file = local
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
            let _ = this.update(cx, |app, cx| {
                app.ask(
                    Confirm {
                        title: format!("Upload {file} to {name}"),
                        body: format!(
                            "Copies {file} ({}) into {} in {name}. A file with the same name there is replaced.",
                            format::bytes(size),
                            files::join(&dir, &file),
                        ),
                        confirm_label: "Upload".into(),
                        action: PendingAction::Upload {
                            container: id,
                            dir,
                            local,
                        },
                    },
                    cx,
                );
            });
        })
        .detach();
    }

    /// Copies `local` into `dir` of `container` (confirmed), then relists.
    pub fn upload_file(
        &mut self,
        container: String,
        dir: String,
        local: PathBuf,
        cx: &mut Context<Self>,
    ) {
        let name = local
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        self.set_transfer(Some(format!("Uploading {name}…")), cx);
        let target = container.clone();
        self.with_engine(
            cx,
            move |engine| async move { files::upload_file(&engine, &target, &dir, &local).await },
            move |app, result, cx| {
                app.set_transfer(None, cx);
                let message = result
                    .as_ref()
                    .map(|path| format!("Uploaded {path}"))
                    .unwrap_or_default();
                app.report(&result, message, cx);
                let shown = app
                    .state
                    .detail
                    .as_ref()
                    .filter(|d| d.id == container && d.tab == DetailTab::Files)
                    .map(|d| d.files.path.clone());
                if let Some(path) = shown {
                    app.load_dir(path, cx);
                }
                app.refresh(Section::Containers, cx);
            },
        );
    }

    fn set_transfer(&mut self, what: Option<String>, cx: &mut Context<Self>) {
        self.update_detail(|d| d.files.transfer = what, cx);
    }
}

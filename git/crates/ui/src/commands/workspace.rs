//! Startup, settings, and repository tabs.

use crate::app::GitView;
use crate::embed::GitViewEvent;
use ferrumgit_domain::{RepoEntry, RepoId, WorkspaceState};
use gpui::Context;
use std::path::PathBuf;

impl GitView {
    /// Loads settings, repositories, profiles and open tabs. Never fails hard.
    pub fn bootstrap(&mut self, cx: &mut Context<Self>) {
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move {
                let s = &services.store;
                Ok::<_, String>((
                    s.load_settings().await,
                    s.list_repos().await,
                    s.list_profiles().await,
                    s.load_workspace().await,
                    services.git.version().await,
                ))
            },
            |app, loaded, cx| {
                let mut ws = WorkspaceState::default();
                match loaded {
                    Ok((settings, repos, profiles, workspace, version)) => {
                        let mut errors = Vec::new();
                        match settings {
                            Ok(s) => app.state.settings = s,
                            Err(e) => errors.push(format!("settings: {e}")),
                        }
                        match repos {
                            // Keep repositories opened while this was loading.
                            Ok(r) => {
                                let mut repos = std::mem::take(&mut app.state.repos);
                                let fresh: Vec<_> = r
                                    .into_iter()
                                    .filter(|e| {
                                        !repos.iter().any(|o| o.id == e.id || o.path == e.path)
                                    })
                                    .collect();
                                repos.extend(fresh);
                                app.state.repos = repos;
                            }
                            Err(e) => errors.push(format!("repositories: {e}")),
                        }
                        match profiles {
                            Ok(p) => app.state.profiles = p,
                            Err(e) => errors.push(format!("profiles: {e}")),
                        }
                        match workspace {
                            Ok(w) => ws = w,
                            Err(e) => errors.push(format!("open tabs: {e}")),
                        }
                        if let Err(e) = version {
                            errors.push(e.to_string());
                        }
                        if !errors.is_empty() {
                            app.notify(format!("Could not load {}", errors.join(", ")), true, cx);
                        }
                    }
                    Err(e) => app.notify(e, true, cx),
                }
                app.state.loaded = true;
                // A single-repository view restores nothing; a repository
                // opened before this finished stays shown.
                if !app.options.show_repo_tabs || app.state.active.is_some() {
                    if let Some(id) = app.state.active
                        && !app.state.open.contains(&id)
                    {
                        app.state.open.push(id);
                    }
                    return;
                }
                let known: Vec<RepoId> = app.state.repos.iter().map(|r| r.id).collect();
                ws.open.retain(|id| known.contains(id));
                let active = ws.active.filter(|id| ws.open.contains(id));
                app.state.open = ws.open.clone();
                app.state.active = active;
                if let Some(id) = active {
                    app.load(id, cx);
                    app.emit_repo_changed(cx);
                }
            },
        );
    }

    pub fn save_settings(&mut self, cx: &mut Context<Self>) {
        let settings = self.state.settings;
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.store.save_settings(&settings).await },
            |app, result, cx| {
                if let Err(e) = result {
                    app.notify(format!("Could not save settings: {e}"), true, cx);
                }
            },
        );
    }

    fn save_workspace(&mut self, cx: &mut Context<Self>) {
        // The open tabs belong to the FerrumGIT app, not to embedded views.
        if !self.options.show_repo_tabs {
            return;
        }
        let ws = WorkspaceState {
            open: self.state.open.clone(),
            active: self.state.active,
        };
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.store.save_workspace(&ws).await },
            |_, result, _| {
                if let Err(e) = result {
                    tracing::warn!(error = %e, "failed to save workspace");
                }
            },
        );
    }

    /// Switches to a repository tab (`None` = home), loading it on first view.
    pub fn activate(&mut self, id: Option<RepoId>, cx: &mut Context<Self>) {
        let changed = self.state.active != id;
        self.state.active = id;
        if let Some(id) = id {
            if !self.options.show_repo_tabs {
                // One repository at a time.
                let previous = std::mem::take(&mut self.state.open);
                for old in previous.into_iter().filter(|o| *o != id) {
                    self.state.views.remove(&old);
                }
            }
            if !self.state.open.contains(&id) {
                self.state.open.push(id);
            }
            if self
                .state
                .views
                .get(&id)
                .is_none_or(|v| v.data.is_none() && !v.loading)
            {
                self.load(id, cx);
            }
        }
        self.save_workspace(cx);
        if changed {
            self.emit_repo_changed(cx);
        }
        cx.notify();
    }

    fn emit_repo_changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(GitViewEvent::RepoChanged {
            path: self.repo_path(),
        });
    }

    pub fn close_tab(&mut self, id: RepoId, cx: &mut Context<Self>) {
        if !self.release_editor(id, cx) {
            return;
        }
        let open = &mut self.state.open;
        let idx = open.iter().position(|o| *o == id);
        open.retain(|o| *o != id);
        let next = idx.and_then(|i| open.get(i.min(open.len().saturating_sub(1))).copied());
        self.state.views.remove(&id);
        if self.state.active == Some(id) {
            self.activate(next, cx);
        } else {
            self.save_workspace(cx);
        }
    }

    /// Records `entry` in the known list (replacing by id) and opens it.
    pub fn upsert_and_open(&mut self, entry: RepoEntry, cx: &mut Context<Self>) {
        let id = entry.id;
        self.state
            .repos
            .retain(|r| r.id != id && r.path != entry.path);
        self.state.repos.insert(0, entry);
        self.state.views.remove(&id);
        self.activate(Some(id), cx);
        if let Some(page) = self.pending_page.take() {
            self.set_page(id, page, cx);
        }
    }

    /// Opens (and remembers) the repository containing `path`. Embedded,
    /// a folder outside any work tree opens as a folder pane instead.
    pub fn open_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if !self.options.show_repo_tabs && !crate::folder::in_work_tree(&path) {
            self.enter_folder(path, cx);
            return;
        }
        self.folder = None;
        self.add_and_open(path, cx);
    }

    /// Records the repository containing `path` and shows it. A result
    /// that arrives after the view moved on to a folder is dropped (unless
    /// it is the repository picked there).
    pub(crate) fn add_and_open(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let p = path.display().to_string();
        self.spawn_run(
            cx,
            async move { services.add_repo(&p).await },
            move |app, result, cx| {
                if app
                    .folder
                    .as_ref()
                    .is_some_and(|f| f.picked.as_ref() != Some(&path))
                {
                    return;
                }
                match result {
                    Ok(entry) => app.upsert_and_open(entry, cx),
                    Err(e) => app.notify(e, true, cx),
                }
            },
        );
    }

    pub fn pick_and_open(&mut self, cx: &mut Context<Self>) {
        self.pick_path(cx, "Open repository", true, |app, path, cx| {
            app.open_path(path, cx);
        });
    }

    pub fn pick_and_init(&mut self, cx: &mut Context<Self>) {
        self.pick_path(cx, "Create a new repository in…", true, |app, dir, cx| {
            let services = app.services.clone();
            let p = dir.display().to_string();
            app.spawn_run(
                cx,
                async move { services.init_repo(&p).await },
                |app, result, cx| match result {
                    Ok(entry) => {
                        app.notify(format!("Initialized {}", entry.name), false, cx);
                        app.upsert_and_open(entry, cx);
                    }
                    Err(e) => app.notify(e, true, cx),
                },
            );
        });
    }

    /// Forgets a repository (the files are not touched).
    pub fn forget(&mut self, id: RepoId, cx: &mut Context<Self>) {
        if !self.release_editor(id, cx) {
            return;
        }
        if self.state.open.contains(&id) {
            self.close_tab(id, cx);
        }
        self.state.repos.retain(|r| r.id != id);
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.store.delete_repo(id).await },
            |app, result, cx| {
                if let Err(e) = result {
                    app.notify(e, true, cx);
                }
            },
        );
        cx.notify();
    }
}

/// Opens the repository folder in the OS file manager.
pub fn reveal(path: &str) {
    let opener = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    if let Err(e) = std::process::Command::new(opener).arg(path).spawn() {
        tracing::warn!(error = %e, "could not open file manager");
    }
}

pub fn open_terminal(path: &str) {
    let result = if cfg!(target_os = "macos") {
        std::process::Command::new("open")
            .args(["-a", "Terminal", path])
            .spawn()
    } else {
        std::process::Command::new("x-terminal-emulator")
            .current_dir(path)
            .spawn()
    };
    if let Err(e) = result {
        tracing::warn!(error = %e, "could not open a terminal");
    }
}

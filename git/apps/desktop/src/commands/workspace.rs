//! Startup, settings, and repository tabs.

use super::{background, repo, run};
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumgit_application::AppServices;
use ferrumgit_domain::{RepoEntry, RepoId, WorkspaceState};
use std::path::PathBuf;

/// Loads settings, repositories, profiles and open tabs. Never fails hard.
pub async fn bootstrap(mut state: AppState, services: AppServices) {
    if let Ok(dark) =
        document::eval("return window.matchMedia('(prefers-color-scheme: dark)').matches")
            .join::<bool>()
            .await
    {
        state.system_dark.set(dark);
    }
    let svc = services.clone();
    let loaded = background(&services, async move {
        let s = &svc.store;
        (
            s.load_settings().await,
            s.list_repos().await,
            s.list_profiles().await,
            s.load_workspace().await,
            svc.git.version().await,
        )
    })
    .await;
    let mut ws = WorkspaceState::default();
    match loaded {
        Ok((settings, repos, profiles, workspace, version)) => {
            let mut errors = Vec::new();
            match settings {
                Ok(s) => state.settings.set(s),
                Err(e) => errors.push(format!("settings: {e}")),
            }
            match repos {
                Ok(r) => state.repos.set(r),
                Err(e) => errors.push(format!("repositories: {e}")),
            }
            match profiles {
                Ok(p) => state.profiles.set(p),
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
                state.notify(format!("Could not load {}", errors.join(", ")), true);
            }
        }
        Err(e) => state.notify(e, true),
    }
    let known: Vec<RepoId> = state.repos.peek().iter().map(|r| r.id).collect();
    ws.open.retain(|id| known.contains(id));
    let active = ws.active.filter(|id| ws.open.contains(id));
    state.open.set(ws.open.clone());
    state.active.set(active);
    state.loaded.set(true);
    if let Some(id) = active {
        repo::load(state, services, id);
    }
}

pub fn save_settings(state: AppState, services: AppServices) {
    let settings = *state.settings.peek();
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(&services, async move {
            svc.store.save_settings(&settings).await
        })
        .await
        {
            state.notify(format!("Could not save settings: {e}"), true);
        }
    });
}

fn save_workspace(state: AppState, services: AppServices) {
    let ws = WorkspaceState {
        open: state.open.peek().clone(),
        active: *state.active.peek(),
    };
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(
            &services,
            async move { svc.store.save_workspace(&ws).await },
        )
        .await
        {
            tracing::warn!(error = %e, "failed to save workspace");
        }
    });
}

/// Switches to a repository tab (`None` = home), loading it on first view.
pub fn activate(mut state: AppState, services: AppServices, id: Option<RepoId>) {
    state.active.set(id);
    if let Some(id) = id {
        if !state.open.peek().contains(&id) {
            state.open.write().push(id);
        }
        if state
            .views
            .peek()
            .get(&id)
            .is_none_or(|v| v.data.is_none() && !v.loading)
        {
            repo::load(state, services.clone(), id);
        }
    }
    save_workspace(state, services);
}

pub fn close_tab(mut state: AppState, services: AppServices, id: RepoId) {
    let mut open = state.open.write();
    let idx = open.iter().position(|o| *o == id);
    open.retain(|o| *o != id);
    let next = idx.and_then(|i| open.get(i.min(open.len().saturating_sub(1))).copied());
    drop(open);
    state.views.write().remove(&id);
    if *state.active.peek() == Some(id) {
        activate(state, services, next);
    } else {
        save_workspace(state, services);
    }
}

/// Records `entry` in the known list (replacing by id) and opens it.
pub fn upsert_and_open(mut state: AppState, services: AppServices, entry: RepoEntry) {
    let id = entry.id;
    {
        let mut repos = state.repos.write();
        repos.retain(|r| r.id != id && r.path != entry.path);
        repos.insert(0, entry);
    }
    state.views.write().remove(&id);
    activate(state, services, Some(id));
}

/// Opens (and remembers) the repository containing `path`.
pub fn open_path(state: AppState, services: AppServices, path: PathBuf) {
    let svc = services.clone();
    spawn(async move {
        let p = path.display().to_string();
        match run(&services, async move { svc.add_repo(&p).await }).await {
            Ok(entry) => upsert_and_open(state, services, entry),
            Err(e) => state.notify(e, true),
        }
    });
}

pub fn pick_and_open(state: AppState, services: AppServices) {
    spawn(async move {
        if let Some(dir) = rfd::AsyncFileDialog::new()
            .set_title("Open repository")
            .pick_folder()
            .await
        {
            open_path(state, services, dir.path().to_path_buf());
        }
    });
}

pub fn pick_and_init(state: AppState, services: AppServices) {
    spawn(async move {
        let Some(dir) = rfd::AsyncFileDialog::new()
            .set_title("Create a new repository in…")
            .pick_folder()
            .await
        else {
            return;
        };
        let svc = services.clone();
        let p = dir.path().display().to_string();
        match run(&services, async move { svc.init_repo(&p).await }).await {
            Ok(entry) => {
                state.notify(format!("Initialized {}", entry.name), false);
                upsert_and_open(state, services, entry);
            }
            Err(e) => state.notify(e, true),
        }
    });
}

/// Forgets a repository (the files are not touched).
pub fn forget(mut state: AppState, services: AppServices, id: RepoId) {
    if state.open.peek().contains(&id) {
        close_tab(state, services.clone(), id);
    }
    state.repos.write().retain(|r| r.id != id);
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(&services, async move { svc.store.delete_repo(id).await }).await {
            state.notify(e, true);
        }
    });
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

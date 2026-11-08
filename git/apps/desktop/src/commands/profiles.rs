//! Profile management and per-repository assignment.

use super::run;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumgit_application::{AppServices, PassphraseChange};
use ferrumgit_domain::{Profile, ProfileId, RepoId};

/// Saves `profile`; calls `done` with the stored version.
pub fn save(
    state: AppState,
    services: AppServices,
    profile: Profile,
    change: PassphraseChange,
    done: impl FnOnce(Profile) + 'static,
) {
    let svc = services.clone();
    spawn(async move {
        match run(
            &services,
            async move { svc.save_profile(profile, change).await },
        )
        .await
        {
            Ok(saved) => {
                let mut profiles = state.profiles;
                {
                    let mut list = profiles.write();
                    list.retain(|p| p.id != saved.id);
                    list.push(saved.clone());
                    list.sort_by_key(|p| p.name.to_lowercase());
                }
                state.notify(format!("Saved profile “{}”", saved.name), false);
                done(saved);
            }
            Err(e) => state.notify(e, true),
        }
    });
}

pub fn delete(mut state: AppState, services: AppServices, id: ProfileId) {
    state.profiles.write().retain(|p| p.id != id);
    for r in state.repos.write().iter_mut() {
        if r.profile == Some(id) {
            r.profile = None;
        }
    }
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(&services, async move { svc.delete_profile(id).await }).await {
            state.notify(e, true);
        }
    });
}

/// Assigns `profile` to repository `repo` (identity + SSH key for that project).
pub fn assign(
    mut state: AppState,
    services: AppServices,
    repo: RepoId,
    profile: Option<ProfileId>,
) {
    let Some(entry) = state.entry(repo) else {
        return;
    };
    if let Some(r) = state.repos.write().iter_mut().find(|r| r.id == repo) {
        r.profile = profile;
    }
    let svc = services.clone();
    spawn(async move {
        if let Err(e) = run(&services, async move {
            svc.set_repo_profile(entry, profile).await
        })
        .await
        {
            state.notify(e, true);
        }
    });
}

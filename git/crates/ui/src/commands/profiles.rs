//! Profile management and per-repository assignment.

use crate::app::GitView;
use ferrumgit_application::PassphraseChange;
use ferrumgit_domain::{Profile, ProfileId, RepoId};
use gpui::Context;

impl GitView {
    /// Saves `profile`; calls `done` with the stored version.
    pub fn save_profile(
        &mut self,
        profile: Profile,
        change: PassphraseChange,
        cx: &mut Context<Self>,
        done: impl FnOnce(&mut Self, Profile, &mut Context<Self>) + 'static,
    ) {
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.save_profile(profile, change).await },
            move |app, result, cx| match result {
                Ok(saved) => {
                    let list = &mut app.state.profiles;
                    list.retain(|p| p.id != saved.id);
                    list.push(saved.clone());
                    list.sort_by_key(|p| p.name.to_lowercase());
                    app.notify(format!("Saved profile “{}”", saved.name), false, cx);
                    done(app, saved, cx);
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    pub fn delete_profile(&mut self, id: ProfileId, cx: &mut Context<Self>) {
        self.state.profiles.retain(|p| p.id != id);
        for r in &mut self.state.repos {
            if r.profile == Some(id) {
                r.profile = None;
            }
        }
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.delete_profile(id).await },
            |app, result, cx| {
                if let Err(e) = result {
                    app.notify(e, true, cx);
                }
            },
        );
        cx.notify();
    }

    /// Assigns `profile` to repository `repo` (identity + SSH key for that project).
    pub fn assign_profile(
        &mut self,
        repo: RepoId,
        profile: Option<ProfileId>,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.state.entry(repo) else {
            return;
        };
        if let Some(r) = self.state.repos.iter_mut().find(|r| r.id == repo) {
            r.profile = profile;
        }
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.set_repo_profile(entry, profile).await },
            |app, result, cx| {
                if let Err(e) = result {
                    app.notify(e, true, cx);
                }
            },
        );
        cx.notify();
    }
}

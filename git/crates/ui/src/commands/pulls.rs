//! Pull / merge requests page: forge info, listing, detail, actions.

use crate::app::GitView;
use crate::state::{Busy, Load, Page, PrTab};
use ferrumgit_application::forge::{
    MergeMethod, NewPullRequest, PrFilter, PullRequest, ReviewEvent,
};
use ferrumgit_domain::{RepoForge, RepoId};
use gpui::Context;
use std::sync::Arc;

/// Opens `url` in the default browser (http(s) only).
pub fn open_url(url: &str) {
    if !(url.starts_with("https://") || url.starts_with("http://")) {
        tracing::warn!("refusing to open a non-web URL");
        return;
    }
    super::workspace::reveal(url);
}

impl GitView {
    /// Shows the pull requests page and loads it the first time.
    pub fn open_pull_requests(&mut self, id: RepoId, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| {
            v.page = Page::PullRequests;
            v.file = None;
        });
        if matches!(self.state.view(id).prs.info, Load::Idle | Load::Failed(_)) {
            self.load_forge_info(id, cx);
        }
        cx.notify();
    }

    /// Detects the forge and token state, then lists when a token exists.
    pub fn load_forge_info(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        self.state.update_view(id, |v| v.prs.info = Load::Loading);
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.forge_info(&entry).await },
            move |app, result, cx| {
                let ready = result.as_ref().is_ok_and(|i| i.has_token);
                app.state.update_view(id, |v| {
                    v.prs.info = match result {
                        Ok(info) => Load::Ready(Arc::new(info)),
                        Err(e) => Load::Failed(e),
                    };
                });
                if ready {
                    app.load_pull_requests(id, cx);
                }
            },
        );
    }

    pub fn set_pr_filter(&mut self, id: RepoId, filter: PrFilter, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| v.prs.filter = filter);
        self.load_pull_requests(id, cx);
    }

    pub fn load_pull_requests(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        let filter = self.state.view(id).prs.filter;
        self.state.update_view(id, |v| v.prs.list = Load::Loading);
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.pull_requests(&entry, filter).await },
            move |app, result, cx| {
                let mut reselect = None;
                app.state.update_view(id, |v| {
                    if v.prs.filter != filter {
                        return;
                    }
                    v.prs.list = match result {
                        Ok(list) => {
                            reselect = v
                                .prs
                                .selected
                                .filter(|n| list.prs.iter().any(|p| p.number == *n));
                            Load::Ready(Arc::new(list))
                        }
                        Err(e) => Load::Failed(e),
                    };
                });
                if let Some(n) = reselect {
                    app.select_pull_request(id, n, cx);
                }
            },
        );
    }

    pub fn select_pull_request(&mut self, id: RepoId, number: u64, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        self.state.update_view(id, |v| {
            if v.prs.selected != Some(number) {
                v.prs.changes = Load::Idle;
                v.expanded.clear();
            }
            v.prs.selected = Some(number);
            v.prs.detail = Load::Loading;
        });
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.pull_request(&entry, number).await },
            move |app, result, cx| {
                let mut load_files = false;
                app.state.update_view(id, |v| {
                    if v.prs.selected != Some(number) {
                        return;
                    }
                    let head_changed = match (&result, v.prs.detail.ready()) {
                        (Ok(new), Some(old)) => new.pr.head_sha != old.pr.head_sha,
                        _ => true,
                    };
                    v.prs.detail = match result {
                        Ok(d) => Load::Ready(Arc::new(d)),
                        Err(e) => Load::Failed(e),
                    };
                    if head_changed {
                        v.prs.changes = Load::Idle;
                    }
                    load_files = v.prs.tab == PrTab::Files && v.prs.changes == Load::Idle;
                });
                if load_files {
                    app.load_pr_changes(id, cx);
                }
            },
        );
        cx.notify();
    }

    pub fn set_pr_tab(&mut self, id: RepoId, tab: PrTab, cx: &mut Context<Self>) {
        self.state.update_view(id, |v| v.prs.tab = tab);
        let view = self.state.view(id);
        if tab == PrTab::Files
            && view.prs.changes == Load::Idle
            && view.prs.detail.ready().is_some()
        {
            self.load_pr_changes(id, cx);
        }
        cx.notify();
    }

    fn selected_pr(&self, id: RepoId) -> Option<PullRequest> {
        self.state.view(id).prs.detail.ready().map(|d| d.pr.clone())
    }

    /// Fetches the PR head and diffs it locally against the merge base.
    pub fn load_pr_changes(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let (Some(entry), Some(pr)) = (self.state.entry(id), self.selected_pr(id)) else {
            return;
        };
        let profile = self.state.profile_of(&entry);
        self.state
            .update_view(id, |v| v.prs.changes = Load::Loading);
        let services = self.services.clone();
        let number = pr.number;
        self.spawn_run(
            cx,
            async move {
                services
                    .pull_request_changes(&entry, profile.as_ref(), &pr, None)
                    .await
            },
            move |app, result, _| {
                app.state.update_view(id, |v| {
                    if v.prs.selected == Some(number) {
                        v.prs.changes = match result {
                            Ok(c) => Load::Ready(Arc::new(c)),
                            Err(e) => Load::Failed(e),
                        };
                    }
                });
            },
        );
    }

    /// Runs a forge/git job under the repository's busy indicator, then `done`.
    fn pr_job<F, T>(
        &mut self,
        id: RepoId,
        label: &str,
        future: F,
        done: impl FnOnce(&mut Self, T, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) where
        F: std::future::Future<Output = Result<T, ferrumgit_application::AppError>>
            + Send
            + 'static,
        T: Send + 'static,
    {
        if self.state.is_busy(id) {
            self.notify(
                "Another operation is still running in this repository",
                true,
                cx,
            );
            return;
        }
        self.state.busy.insert(
            id,
            Busy {
                label: label.to_owned(),
                cancel: None,
            },
        );
        self.spawn_run(cx, future, move |app, result, cx| {
            app.state.busy.remove(&id);
            match result {
                Ok(value) => done(app, value, cx),
                Err(e) => app.notify(e, true, cx),
            }
        });
        cx.notify();
    }

    pub fn checkout_pull_request(&mut self, id: RepoId, cx: &mut Context<Self>) {
        let (Some(entry), Some(pr)) = (self.state.entry(id), self.selected_pr(id)) else {
            return;
        };
        let profile = self.state.profile_of(&entry);
        let services = self.services.clone();
        self.pr_job(
            id,
            "Checking out pull request",
            async move {
                services
                    .checkout_pull_request(&entry, profile.as_ref(), &pr, None)
                    .await
            },
            move |app, message, cx| {
                app.notify(message, false, cx);
                app.load(id, cx);
            },
            cx,
        );
    }

    pub fn create_pull_request(&mut self, new: NewPullRequest, cx: &mut Context<Self>) {
        let Some(entry) = self.state.active_entry() else {
            return;
        };
        let id = entry.id;
        let profile = self.state.profile_of(&entry);
        let services = self.services.clone();
        self.pr_job(
            id,
            "Creating pull request",
            async move {
                services
                    .create_pull_request(&entry, profile.as_ref(), new, None)
                    .await
            },
            move |app, pr, cx| {
                app.notify(format!("Created {}", pr.web_url), false, cx);
                app.load(id, cx);
                app.state.update_view(id, |v| {
                    v.page = Page::PullRequests;
                    v.prs.filter = PrFilter::All;
                });
                app.load_pull_requests(id, cx);
                app.select_pull_request(id, pr.number, cx);
            },
            cx,
        );
    }

    /// Plain comment (`ReviewEvent::Comment`) or a review verdict.
    pub fn submit_review(
        &mut self,
        number: u64,
        event: ReviewEvent,
        body: String,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.state.active_entry() else {
            return;
        };
        let id = entry.id;
        let services = self.services.clone();
        self.pr_job(
            id,
            "Sending review",
            async move {
                match event {
                    ReviewEvent::Comment => {
                        services.comment_pull_request(&entry, number, &body).await
                    }
                    _ => {
                        services
                            .review_pull_request(&entry, number, event, &body)
                            .await
                    }
                }
            },
            move |app, (), cx| {
                let msg = match event {
                    ReviewEvent::Comment => "Comment posted",
                    ReviewEvent::Approve => "Approved",
                    ReviewEvent::RequestChanges => "Changes requested",
                };
                app.notify(msg, false, cx);
                app.select_pull_request(id, number, cx);
            },
            cx,
        );
    }

    pub fn merge_pull_request(&mut self, number: u64, method: MergeMethod, cx: &mut Context<Self>) {
        let Some(entry) = self.state.active_entry() else {
            return;
        };
        let id = entry.id;
        let Some(sha) = self
            .selected_pr(id)
            .filter(|p| p.number == number)
            .map(|p| p.head_sha)
        else {
            return;
        };
        let services = self.services.clone();
        self.pr_job(
            id,
            "Merging",
            async move {
                services
                    .merge_pull_request(&entry, number, method, &sha)
                    .await
            },
            move |app, (), cx| {
                app.notify("Merged on the server. Pull to get the result.", false, cx);
                app.state.update_view(id, |v| {
                    v.prs.selected = None;
                    v.prs.detail = Load::Idle;
                });
                app.load_pull_requests(id, cx);
            },
            cx,
        );
    }

    /// Token from the onboarding dialog; reloads the page on success.
    pub fn save_forge_token(
        &mut self,
        id: RepoId,
        host: String,
        token: String,
        cx: &mut Context<Self>,
    ) {
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.set_forge_token(&host, &token).await },
            move |app, result, cx| match result {
                Ok(()) => {
                    app.notify("Token saved in the keychain", false, cx);
                    app.load_forge_info(id, cx);
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    pub fn use_gh_cli(&mut self, id: RepoId, host: String, cx: &mut Context<Self>) {
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.use_gh_cli(&host).await },
            move |app, result, cx| match result {
                Ok(()) => {
                    app.notify("Using the GitHub CLI login", false, cx);
                    app.load_forge_info(id, cx);
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    pub fn clear_forge_token(&mut self, id: RepoId, host: String, cx: &mut Context<Self>) {
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.clear_forge_token(&host).await },
            move |app, result, cx| match result {
                Ok(()) => {
                    app.notify("Token removed", false, cx);
                    app.state.update_view(id, |v| v.prs = Default::default());
                    app.load_forge_info(id, cx);
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }

    /// Saves forge overrides for a repository and re-detects.
    pub fn save_repo_forge(&mut self, id: RepoId, forge: RepoForge, cx: &mut Context<Self>) {
        let Some(entry) = self.state.entry(id) else {
            return;
        };
        let services = self.services.clone();
        self.spawn_run(
            cx,
            async move { services.set_repo_forge(entry, forge).await },
            move |app, result, cx| match result {
                Ok(entry) => {
                    if let Some(slot) = app.state.repos.iter_mut().find(|r| r.id == entry.id) {
                        *slot = entry;
                    }
                    app.state.update_view(id, |v| v.prs = Default::default());
                    app.load_forge_info(id, cx);
                }
                Err(e) => app.notify(e, true, cx),
            },
        );
    }
}

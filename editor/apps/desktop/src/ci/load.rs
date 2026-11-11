//! Talking to GitHub: every request runs on the background runtime and
//! lands back on the app, unless the user switched repository meanwhile.

use crate::ci::detail::{LogView, RunDetail};
use crate::ci::{CiView, Screen};
use chrono::Utc;
use ferrum_ui::prelude::*;
use ferrumci_core::logs::JobLog;
use ferrumci_core::watch::{self, Finished};
use ferrumci_core::{Client, ForgeError, ForgeResult, Job, PAGE_SIZE, RunsPage, WorkflowRun};
use std::future::Future;
use std::sync::Arc;
use std::time::{Duration, Instant};

type Refreshed = (RunsPage, Vec<WorkflowRun>, Option<Vec<Job>>);

impl CiView {
    /// Runs `make(client)` in the background and hands the result to `done`.
    pub(crate) fn spawn_forge<T, Fut>(
        &mut self,
        cx: &mut Context<Self>,
        make: impl FnOnce(Client) -> Fut,
        done: impl FnOnce(&mut Self, ForgeResult<T>, &mut Context<Self>) + 'static,
    ) where
        T: Send + 'static,
        Fut: Future<Output = ForgeResult<T>> + Send + 'static,
    {
        let Some(client) = self.client.clone() else {
            return;
        };
        let future = make(client);
        let services = self.services.clone();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let result = services
                .run(future)
                .await
                .unwrap_or_else(|e| Err(ForgeError::Invalid(e)));
            this.update(cx, |this, cx| {
                if this.generation == generation {
                    done(this, result, cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Shows a request error; a rejected token sends the user to sign in.
    pub(crate) fn fail(&mut self, error: &ForgeError) {
        if let ForgeError::Unauthorized { .. } = error
            && let Some(client) = self.client.take()
        {
            self.screen = Screen::SignIn {
                repo: client.repo,
                error: Some(error.to_string()),
            };
        }
        self.error = Some(error.to_string());
    }

    /// Reloads the run list from scratch (filters changed).
    pub(crate) fn reload_runs(&mut self, cx: &mut Context<Self>) {
        self.runs_loading = false;
        self.refresh(cx);
    }

    /// Runs, watched runs and the open run's jobs.
    pub(crate) fn refresh(&mut self, cx: &mut Context<Self>) {
        if self.client.is_none() || self.runs_loading {
            return;
        }
        self.runs_loading = true;
        self.runs_seq += 1;
        let seq = self.runs_seq;
        let filter = self.filter.clone();
        let detail_id = self.detail.as_ref().map(|d| d.run.id);
        let mut extra_ids = self.watcher.ids();
        extra_ids.extend(detail_id);
        cx.notify();
        self.spawn_forge(
            cx,
            move |c| async move {
                let page = c.actions.runs(&filter, PAGE_SIZE).await?;
                let mut extra = Vec::new();
                for id in extra_ids.into_iter().take(8) {
                    let known = page.workflow_runs.iter().chain(&extra).any(|r| r.id == id);
                    if known {
                        continue;
                    }
                    match c.actions.run(id).await {
                        Ok(run) => extra.push(run),
                        Err(e) => tracing::debug!(id, error = %e, "watched run lookup failed"),
                    }
                }
                let jobs = match detail_id {
                    Some(id) => Some(c.actions.jobs(id).await?),
                    None => None,
                };
                Ok::<Refreshed, ForgeError>((page, extra, jobs))
            },
            move |this, result, cx| {
                if this.runs_seq == seq {
                    this.on_refreshed(result, cx);
                }
            },
        );
    }

    fn on_refreshed(&mut self, result: ForgeResult<Refreshed>, cx: &mut Context<Self>) {
        self.runs_loading = false;
        match result {
            Ok((page, extra, jobs)) => {
                self.error = None;
                self.last_refresh = Some(Utc::now());
                self.runs_total = page.total_count;
                self.runs = page.workflow_runs;
                if let Some(detail) = self.detail.as_mut() {
                    let id = detail.run.id;
                    if let Some(run) = self.runs.iter().chain(&extra).find(|r| r.id == id) {
                        detail.run = run.clone();
                    }
                    if let Some(jobs) = jobs {
                        detail.set_jobs(jobs);
                    }
                }
                // A selected job that just finished gets its log.
                if let Some(job) = self.detail.as_ref().and_then(|d| d.job) {
                    self.select_job(job, cx);
                }
                let all: Vec<WorkflowRun> = self.runs.iter().chain(&extra).cloned().collect();
                let finished = self.watcher.observe(&all);
                self.announce(finished, cx);
            }
            Err(e) => self.fail(&e),
        }
        self.schedule_refresh();
    }

    pub(crate) fn schedule_refresh(&mut self) {
        let rate = self.client.as_ref().and_then(|c| c.actions.rate());
        let wait = watch::next_refresh(self.is_active(), rate, Utc::now().timestamp());
        self.next_refresh = Instant::now() + wait;
    }

    /// Refresh shortly (after an action changed something).
    pub(crate) fn refresh_soon(&mut self) {
        self.next_refresh = Instant::now() + Duration::from_secs(3);
    }

    fn announce(&mut self, finished: Vec<Finished>, cx: &mut Context<Self>) {
        for f in finished {
            let headline = f.headline();
            let body = f.title.clone();
            let notify = self.services.notify.clone();
            let (h, b) = (headline.clone(), body.clone());
            let services = self.services.clone();
            cx.background_spawn(async move {
                services.blocking(move || notify(&h, &b)).await.ok();
            })
            .detach();
            let notice = if f.state == ferrumci_core::RunState::Failure {
                Notice::error(format!("{headline}: {body}"))
            } else {
                Notice::info(format!("{headline}: {body}"))
            };
            self.show_notice(notice, cx);
        }
    }

    pub(crate) fn load_workflows(&mut self, cx: &mut Context<Self>) {
        self.spawn_forge(
            cx,
            |c| async move { c.actions.workflows().await },
            |this, result, _| match result {
                Ok(list) => this.workflows = list,
                Err(e) => this.fail(&e),
            },
        );
    }

    pub(crate) fn load_prs(&mut self, cx: &mut Context<Self>) {
        if self.prs_loading {
            return;
        }
        self.prs_loading = true;
        self.spawn_forge(
            cx,
            |c| async move { c.pull_requests().await },
            |this, result, _| {
                this.prs_loading = false;
                this.prs_loaded_at = Some(Instant::now());
                match result {
                    Ok(prs) => this.prs = prs,
                    Err(e) => this.fail(&e),
                }
            },
        );
    }

    pub(crate) fn open_run(&mut self, run: WorkflowRun, cx: &mut Context<Self>) {
        let id = run.id;
        self.detail = Some(RunDetail::new(run));
        self.log_search.update(cx, |s, cx| s.set_text("", cx));
        cx.notify();
        self.spawn_forge(
            cx,
            move |c| async move { c.actions.jobs(id).await },
            move |this, result, cx| match result {
                Ok(jobs) => {
                    let Some(detail) = this.detail.as_mut().filter(|d| d.run.id == id) else {
                        return;
                    };
                    detail.set_jobs(jobs);
                    if let Some(job) = detail.default_job() {
                        this.select_job(job, cx);
                    }
                }
                Err(e) => this.fail(&e),
            },
        );
    }

    pub(crate) fn close_run(&mut self, cx: &mut Context<Self>) {
        self.detail = None;
        cx.notify();
    }

    /// Shows a job's steps and, once it has finished, its log.
    pub(crate) fn select_job(&mut self, job_id: u64, cx: &mut Context<Self>) {
        let Some(detail) = self.detail.as_mut() else {
            return;
        };
        detail.job = Some(job_id);
        let finished = detail
            .jobs
            .iter()
            .find(|j| j.id == job_id)
            .is_some_and(|j| !j.state().is_active());
        if !finished {
            detail.log = None;
            cx.notify();
            return;
        }
        if detail.log.as_ref().is_some_and(|l| l.job_id == job_id) {
            return;
        }
        detail.log = Some(LogView::loading(job_id));
        self.log_search.update(cx, |s, cx| s.set_text("", cx));
        cx.notify();
        self.spawn_forge(
            cx,
            move |c| async move {
                let text = c.actions.job_log(job_id).await?;
                Ok(Arc::new(JobLog::parse(&text)))
            },
            move |this, result, _| {
                let Some(view) = this
                    .detail
                    .as_mut()
                    .and_then(|d| d.log.as_mut())
                    .filter(|l| l.job_id == job_id)
                else {
                    return;
                };
                match result {
                    Ok(log) => view.loaded(log),
                    Err(e) => {
                        view.loading = false;
                        view.error = Some(e.to_string());
                    }
                }
            },
        );
    }

    pub(crate) fn search_log(&mut self, cx: &mut Context<Self>) {
        let query = self.log_search.read(cx).text().to_owned();
        if let Some(view) = self.detail.as_mut().and_then(|d| d.log.as_mut()) {
            view.search(&query);
        }
        cx.notify();
    }

    pub(crate) fn next_match(&mut self, step: isize, cx: &mut Context<Self>) {
        if let Some(view) = self.detail.as_mut().and_then(|d| d.log.as_mut()) {
            view.step_match(step);
        }
        cx.notify();
    }

    pub(crate) fn rerun(&mut self, failed_only: bool, cx: &mut Context<Self>) {
        let Some(run) = self.detail.as_ref().map(|d| d.run.clone()) else {
            return;
        };
        let id = run.id;
        self.spawn_forge(
            cx,
            move |c| async move { c.actions.rerun(id, failed_only).await },
            move |this, result, cx| match result {
                Ok(()) => {
                    this.watcher.watch_id(id);
                    this.refresh_soon();
                    let what = if failed_only {
                        "Failed jobs of"
                    } else {
                        "All jobs of"
                    };
                    this.show_notice(
                        Notice::info(format!(
                            "{what} {} #{} will run again. You'll be told when it finishes.",
                            run.workflow_name(),
                            run.run_number
                        )),
                        cx,
                    );
                }
                Err(e) => this.show_notice(Notice::error(e.to_string()), cx),
            },
        );
    }

    pub(crate) fn cancel(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.detail.as_ref().map(|d| d.run.id) else {
            return;
        };
        self.spawn_forge(
            cx,
            move |c| async move { c.actions.cancel(id).await },
            move |this, result, cx| match result {
                Ok(()) => {
                    this.watcher.watch_id(id);
                    this.refresh_soon();
                    this.show_notice(Notice::info("Cancelling the run…"), cx);
                }
                Err(e) => this.show_notice(Notice::error(e.to_string()), cx),
            },
        );
    }

    pub(crate) fn toggle_watch(&mut self, cx: &mut Context<Self>) {
        let Some(run) = self.detail.as_ref().map(|d| d.run.clone()) else {
            return;
        };
        if self.watcher.is_watching(run.id) {
            self.watcher.unwatch(run.id);
        } else {
            self.watcher.watch(&run);
        }
        cx.notify();
    }

    /// Saves the pasted token (FerrumGIT's keychain entry) and connects.
    pub(crate) fn save_token(&mut self, cx: &mut Context<Self>) {
        let Screen::SignIn { repo, .. } = &self.screen else {
            return;
        };
        let repo = repo.clone();
        let token = self.token_input.read(cx).text().to_owned();
        let services = self.services.clone();
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let tokens = services.tokens.clone();
            let host = repo.host.clone();
            let saved = services
                .blocking(move || tokens.save(&host, &token))
                .await
                .and_then(|r| r);
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match saved {
                    Ok(()) => {
                        this.token_input.update(cx, |t, cx| t.set_text("", cx));
                        this.error = None;
                        this.screen = Screen::Connecting(repo.clone());
                        this.connect(repo, cx);
                    }
                    Err(e) => {
                        this.screen = Screen::SignIn {
                            repo,
                            error: Some(e),
                        }
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Looks for a token again (after `gh auth login` or setting `GITHUB_TOKEN`).
    pub(crate) fn retry_sign_in(&mut self, cx: &mut Context<Self>) {
        if let Screen::SignIn { repo, .. } = &self.screen {
            let repo = repo.clone();
            // Cleared by the first successful refresh.
            self.error = Some("Still looking for a token: none was found last time.".into());
            self.screen = Screen::Connecting(repo.clone());
            self.connect(repo, cx);
            cx.notify();
        }
    }
}

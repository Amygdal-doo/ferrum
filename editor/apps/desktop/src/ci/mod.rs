//! CI in the editor: GitHub Actions runs, job logs, workflow dispatch and
//! pull-request checks for the open project's GitHub repository, using
//! `ferrumci-core`. One [`CiView`] per window, created the first time the
//! CI page opens (⇧⌘I); the workspace hands it the project root.

mod detail;
mod dispatch;
mod load;
mod logview;
mod pulls;
mod runs;
pub(crate) mod services;
mod signin;
mod view;
mod widgets;

use ferrum_ui::prelude::*;
use ferrumci_core::auth::TokenOrigin;
use ferrumci_core::repo::{self, RepoStatus};
use ferrumci_core::watch::Watcher;
use ferrumci_core::{Client, ForgeRepo, PullRequest, RateLimits, RunFilter, Workflow, WorkflowRun};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use detail::RunDetail;
use dispatch::DispatchForm;
pub use services::Services;

/// Key context of the CI page (⌘R refresh and ⌘F log search bind here).
pub const KEY_CONTEXT: &str = "FerrumCi";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tab {
    Runs,
    PullRequests,
}

pub enum Screen {
    /// No GitHub repository to show.
    Empty {
        title: String,
        detail: String,
    },
    GitLab(ForgeRepo),
    /// Looking up a token.
    Connecting(ForgeRepo),
    SignIn {
        repo: ForgeRepo,
        error: Option<String>,
    },
    Ready,
}

/// Asks the workspace to open another project (a recent GitHub one).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CiEvent {
    OpenProject(PathBuf),
}

impl EventEmitter<CiEvent> for CiView {}

pub struct CiView {
    pub(crate) services: Services,
    pub(crate) project: Option<PathBuf>,
    /// The checked-out branch of the project folder.
    pub(crate) local_branch: Option<String>,
    pub(crate) screen: Screen,
    pub(crate) client: Option<Client>,
    pub(crate) token_origin: Option<TokenOrigin>,
    /// Bumped on every repository switch; late answers for an old one are dropped.
    pub(crate) generation: u64,
    pub(crate) tab: Tab,
    pub(crate) runs: Vec<WorkflowRun>,
    pub(crate) runs_total: u64,
    pub(crate) runs_loading: bool,
    /// Only the latest run-list request is applied.
    pub(crate) runs_seq: u64,
    pub(crate) workflows: Vec<Workflow>,
    pub(crate) filter: RunFilter,
    pub(crate) branch_input: Entity<TextInput>,
    pub(crate) detail: Option<RunDetail>,
    pub(crate) prs: Vec<PullRequest>,
    pub(crate) prs_loading: bool,
    pub(crate) prs_loaded_at: Option<Instant>,
    pub(crate) watcher: Watcher,
    pub(crate) dispatch: Option<DispatchForm>,
    pub(crate) token_input: Entity<TextInput>,
    pub(crate) log_search: Entity<TextInput>,
    pub(crate) last_refresh: Option<chrono::DateTime<chrono::Utc>>,
    pub(crate) error: Option<String>,
    pub(crate) next_refresh: Instant,
    pub(crate) github_projects: Vec<(PathBuf, ForgeRepo)>,
    pub(crate) notice: Option<Notice>,
    /// Frames drawn (headless tests check views actually render).
    #[cfg(test)]
    pub(crate) frames: u64,
    notice_task: Option<Task<()>>,
    focus_handle: FocusHandle,
    _ticker: Task<()>,
    _subscriptions: Vec<Subscription>,
}

const NO_PROJECT: &str = "Open a folder whose git remote is on GitHub (⌘⇧O).";

impl CiView {
    pub fn new(services: Services, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let branch_input = cx.new(|cx| TextInput::new(window, cx).placeholder("All branches"));
        let token_input = cx.new(|cx| {
            TextInput::new(window, cx)
                .masked(true)
                .placeholder("ghp_… or github_pat_…")
        });
        let log_search = cx.new(|cx| TextInput::new(window, cx).placeholder("Search log"));
        let subscriptions = vec![
            cx.subscribe(&branch_input, |this: &mut Self, _, event, cx| match event {
                TextInputEvent::Submit | TextInputEvent::Blurred => this.apply_branch_input(cx),
                TextInputEvent::Changed => {}
            }),
            cx.subscribe(&token_input, |this: &mut Self, _, event, cx| {
                if *event == TextInputEvent::Submit {
                    this.save_token(cx);
                }
            }),
            cx.subscribe(&log_search, |this: &mut Self, _, event, cx| match event {
                TextInputEvent::Changed => this.search_log(cx),
                TextInputEvent::Submit => this.next_match(1, cx),
                TextInputEvent::Blurred => {}
            }),
        ];
        let ticker = cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                if this.update(cx, |this, cx| this.tick(cx)).is_err() {
                    break;
                }
            }
        });
        Self {
            services,
            project: None,
            local_branch: None,
            screen: Screen::Empty {
                title: "No project open".into(),
                detail: NO_PROJECT.into(),
            },
            client: None,
            token_origin: None,
            generation: 0,
            tab: Tab::Runs,
            runs: Vec::new(),
            runs_total: 0,
            runs_loading: false,
            runs_seq: 0,
            workflows: Vec::new(),
            filter: RunFilter::default(),
            branch_input,
            detail: None,
            prs: Vec::new(),
            prs_loading: false,
            prs_loaded_at: None,
            watcher: Watcher::default(),
            dispatch: None,
            token_input,
            log_search,
            last_refresh: None,
            error: None,
            next_refresh: Instant::now() + Duration::from_secs(3600),
            github_projects: Vec::new(),
            notice: None,
            #[cfg(test)]
            frames: 0,
            notice_task: None,
            focus_handle: cx.focus_handle(),
            _ticker: ticker,
            _subscriptions: subscriptions,
        }
    }

    /// Shows CI for the editor's project (`None`: no folder open). The
    /// same project again keeps what is loaded.
    pub fn set_project(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        self.github_projects = repo::github_projects(&(self.services.recent)());
        match path {
            Some(path) => {
                let root = repo::git_root(&path).unwrap_or(path);
                if self.project.as_ref() != Some(&root) {
                    self.open_project(root, cx);
                }
            }
            None => {
                self.project = None;
                self.local_branch = None;
                self.generation += 1;
                self.client = None;
                self.screen = Screen::Empty {
                    title: "No project open".into(),
                    detail: NO_PROJECT.into(),
                };
                cx.notify();
            }
        }
    }

    pub fn open_project(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let root = repo::git_root(&path).unwrap_or(path);
        self.local_branch = ferrum_ui::project::git_branch(&root);
        self.project = Some(root.clone());
        let status = repo::detect(&root);
        self.set_repo(status, cx);
    }

    /// Switches to `status`'s repository and starts loading it.
    pub fn set_repo(&mut self, status: RepoStatus, cx: &mut Context<Self>) {
        self.generation += 1;
        self.client = None;
        self.token_origin = None;
        self.runs.clear();
        self.runs_total = 0;
        self.runs_loading = false;
        self.workflows.clear();
        self.detail = None;
        self.prs.clear();
        self.prs_loading = false;
        self.prs_loaded_at = None;
        self.dispatch = None;
        self.watcher = Watcher::default();
        self.error = None;
        self.last_refresh = None;
        self.filter = RunFilter {
            branch: self.local_branch.clone(),
            ..RunFilter::default()
        };
        let branch = self.local_branch.clone().unwrap_or_default();
        self.branch_input
            .update(cx, |input, cx| input.set_text(branch, cx));
        let folder = self
            .project
            .as_deref()
            .map(ferrum_ui::project::project_name)
            .unwrap_or_default();
        self.screen = match status {
            RepoStatus::GitHub(repo) => {
                self.connect(repo.clone(), cx);
                Screen::Connecting(repo)
            }
            RepoStatus::GitLab(repo) => Screen::GitLab(repo),
            RepoStatus::NotGit => Screen::Empty {
                title: format!("{folder} is not a git repository"),
                detail: NO_PROJECT.into(),
            },
            RepoStatus::NoForge => Screen::Empty {
                title: format!("{folder} has no GitHub remote"),
                detail: "The CI page shows GitHub Actions for repositories hosted on GitHub."
                    .into(),
            },
        };
        cx.notify();
    }

    pub(crate) fn repo(&self) -> Option<&ForgeRepo> {
        match &self.screen {
            Screen::GitLab(r) | Screen::Connecting(r) | Screen::SignIn { repo: r, .. } => Some(r),
            Screen::Ready => self.client.as_ref().map(|c| &c.repo),
            Screen::Empty { .. } => None,
        }
    }

    /// Finds a token for `repo`, then loads it, or asks the user to sign in.
    pub(crate) fn connect(&mut self, repo: ForgeRepo, cx: &mut Context<Self>) {
        let services = self.services.clone();
        let generation = self.generation;
        let host = repo.host.clone();
        cx.spawn(async move |this, cx| {
            let tokens = services.tokens.clone();
            let found = services.blocking(move || tokens.find(&host)).await;
            this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match found {
                    Ok(Some(found)) => {
                        this.token_origin = Some(found.origin);
                        this.client = Some(Client::new(
                            repo,
                            this.services.transport.clone(),
                            found.token,
                            Arc::new(RateLimits::default()),
                        ));
                        this.screen = Screen::Ready;
                        this.refresh(cx);
                        this.load_workflows(cx);
                    }
                    Ok(None) => this.screen = Screen::SignIn { repo, error: None },
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

    pub(crate) fn show_notice(&mut self, notice: Notice, cx: &mut Context<Self>) {
        self.notice = Some(notice);
        self.notice_task = Some(Notice::dismiss_later(cx, |this: &mut Self, _| {
            this.notice = None;
        }));
        cx.notify();
    }

    fn apply_branch_input(&mut self, cx: &mut Context<Self>) {
        let text = self.branch_input.read(cx).text().trim().to_owned();
        let branch = (!text.is_empty()).then_some(text);
        if branch != self.filter.branch {
            self.filter.branch = branch;
            self.reload_runs(cx);
        }
    }

    /// Sets the branch filter (and its input).
    pub(crate) fn set_branch_filter(&mut self, branch: Option<String>, cx: &mut Context<Self>) {
        let text = branch.clone().unwrap_or_default();
        self.branch_input
            .update(cx, |input, cx| input.set_text(text, cx));
        self.filter.branch = branch;
        self.reload_runs(cx);
    }

    /// Something in view is still queued or running.
    pub(crate) fn is_active(&self) -> bool {
        self.runs.iter().any(|r| r.state().is_active())
            || self
                .detail
                .as_ref()
                .is_some_and(|d| d.run.state().is_active())
            || !self.watcher.is_empty()
    }

    fn tick(&mut self, cx: &mut Context<Self>) {
        if !matches!(self.screen, Screen::Ready) {
            return;
        }
        if self.is_active() {
            cx.notify(); // elapsed timers
        }
        if Instant::now() >= self.next_refresh {
            self.refresh(cx);
            let stale = self
                .prs_loaded_at
                .is_none_or(|t| t.elapsed() > Duration::from_secs(120));
            if self.tab == Tab::PullRequests && stale {
                self.load_prs(cx);
            }
        }
    }

    /// `ci:refresh` (⌘R on the page): runs, and pull requests on that tab.
    pub fn refresh_all(&mut self, cx: &mut Context<Self>) {
        self.refresh(cx);
        if self.tab == Tab::PullRequests {
            self.load_prs(cx);
        }
    }

    /// `ci:search-log` (⌘F on the page) when a job log is open. Returns
    /// whether there was a log to search.
    pub fn focus_log_search(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if self.detail.as_ref().is_some_and(|d| d.log.is_some()) {
            self.log_search.update(cx, |s, cx| s.focus(window, cx));
            return true;
        }
        false
    }

    pub(crate) fn set_tab(&mut self, tab: Tab, cx: &mut Context<Self>) {
        self.tab = tab;
        if tab == Tab::PullRequests && self.prs_loaded_at.is_none() {
            self.load_prs(cx);
        }
        cx.notify();
    }
}

impl Focusable for CiView {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

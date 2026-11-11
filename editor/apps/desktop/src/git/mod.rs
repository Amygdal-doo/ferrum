//! Git in the editor: the project's working tree status, staging, commit,
//! pull / push and diffs, using FerrumGIT's git layer (`ferrumgit-git`).
//! One [`GitState`] per window feeds the sidebar panel and the full Git
//! view; everything else (history graph, PRs, profiles, stashes…) stays in
//! FerrumGIT, which "Open in FerrumGIT" launches.

mod diff;
mod panel;
mod view;

use std::future::Future;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use ferrum_ui::prelude::*;
use ferrum_ui::{TextInput, TextInputEvent};
use ferrumgit_domain::{
    ChangeKind, Commit, CommitDetails, FileDiff, FileStatus, Oid, PullMode, RepoOp, WorkingStatus,
};
use ferrumgit_git::{Git, GitError, OpContext, Repo};
use gpui::Global;

pub use diff::DiffRow;
pub use panel::{GitPanel, GitPanelEvent};
pub use view::GitView;

/// Commits shown in the Git view.
const HISTORY: usize = 200;
/// Saves and window focus refresh at most this often.
const REFRESH_DEBOUNCE: Duration = Duration::from_millis(300);

/// Runs git (async, on tokio) for every window.
pub struct GitService {
    runtime: Arc<tokio::runtime::Runtime>,
    git: Git,
}

impl Global for GitService {}

impl GitService {
    fn new() -> std::io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("ferrumeditor-git")
            .enable_all()
            .build()?;
        Ok(Self {
            runtime: Arc::new(runtime),
            git: Git::new(),
        })
    }

    /// Runs `job` on the git runtime; resolves to `None` if it was dropped.
    pub fn run<T, F>(
        cx: &mut App,
        job: impl FnOnce(Git) -> F,
    ) -> impl Future<Output = Option<T>> + 'static
    where
        T: Send + 'static,
        F: Future<Output = T> + Send + 'static,
    {
        if !cx.has_global::<GitService>() {
            match GitService::new() {
                Ok(service) => cx.set_global(service),
                Err(error) => {
                    eprintln!("ferrumeditor: could not start git runtime: {error}");
                }
            }
        }
        let (tx, rx) = futures::channel::oneshot::channel();
        if let Some(service) = cx.try_global::<GitService>() {
            let work = job(service.git.clone());
            service.runtime.spawn(async move {
                tx.send(work.await).ok();
            });
        }
        async move { rx.await.ok() }
    }
}

/// What the diff side shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitSelection {
    File { path: String, staged: bool },
    Commit(Oid),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitEvent {
    Notice(String),
    Error(String),
}

pub struct GitState {
    root: Option<PathBuf>,
    repo: Option<Repo>,
    /// The folder is not inside a git working tree.
    pub not_a_repo: bool,
    pub loaded: bool,
    pub status: WorkingStatus,
    pub commits: Vec<Commit>,
    pub selection: Option<GitSelection>,
    /// Rows of the selected file's or commit's diff.
    pub diff: Arc<Vec<DiffRow>>,
    pub commit: Option<CommitDetails>,
    /// An operation is running ("Pushing…").
    pub busy: Option<String>,
    pub message: Entity<TextInput>,
    refresh_task: Option<Task<()>>,
    op_task: Option<Task<()>>,
    diff_task: Option<Task<()>>,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<GitEvent> for GitState {}

fn error_text(error: &GitError) -> String {
    error.to_string()
}

impl GitState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let message = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Message (⌘↵ to commit)")
                .multiline(3)
        });
        let subscription = cx.subscribe(&message, |this: &mut Self, _, event, cx| {
            if matches!(event, TextInputEvent::Submit) {
                this.commit(cx);
            }
        });
        Self {
            root: None,
            repo: None,
            not_a_repo: false,
            loaded: false,
            status: WorkingStatus::default(),
            commits: Vec::new(),
            selection: None,
            diff: Arc::default(),
            commit: None,
            busy: None,
            message,
            refresh_task: None,
            op_task: None,
            diff_task: None,
            _subscriptions: vec![subscription],
        }
    }

    pub fn root(&self) -> Option<&Path> {
        self.root.as_deref()
    }

    pub fn set_root(&mut self, root: Option<PathBuf>, cx: &mut Context<Self>) {
        if self.root == root {
            return;
        }
        self.root = root;
        self.repo = None;
        self.not_a_repo = false;
        self.loaded = false;
        self.status = WorkingStatus::default();
        self.commits.clear();
        self.selection = None;
        self.diff = Arc::default();
        self.commit = None;
        self.refresh(false, cx);
    }

    /// Number of changed files (for badges).
    pub fn change_count(&self) -> usize {
        self.status.files.len()
    }

    pub fn branch_label(&self) -> Option<String> {
        let branch = &self.status.branch;
        let name = branch.head.clone().or_else(|| {
            branch
                .oid
                .as_ref()
                .map(|o| o.as_str().chars().take(7).collect())
        })?;
        let mut label = name;
        if branch.ahead > 0 {
            label.push_str(&format!(" ↑{}", branch.ahead));
        }
        if branch.behind > 0 {
            label.push_str(&format!(" ↓{}", branch.behind));
        }
        Some(label)
    }

    /// Re-reads status and history (debounced when `debounce`).
    pub fn refresh(&mut self, debounce: bool, cx: &mut Context<Self>) {
        let Some(root) = self.root.clone() else {
            return;
        };
        let repo = self.repo.clone();
        let work = GitService::run(cx, move |git| async move {
            let repo = match repo {
                Some(repo) => repo,
                None => Repo::open(&git, &root).await?,
            };
            let status = repo.status().await?;
            let has_commits = status.branch.oid.is_some();
            let (commits, _) = repo.log(HISTORY, has_commits).await?;
            Ok::<_, GitError>((repo, status, commits))
        });
        self.refresh_task = Some(cx.spawn(async move |this, cx| {
            if debounce {
                cx.background_executor().timer(REFRESH_DEBOUNCE).await;
            }
            let Some(result) = work.await else { return };
            this.update(cx, |this, cx| {
                this.loaded = true;
                match result {
                    Ok((repo, status, commits)) => {
                        this.repo = Some(repo);
                        this.not_a_repo = false;
                        this.status = status;
                        this.commits = commits;
                        this.reload_selection(cx);
                    }
                    Err(GitError::NotARepo(_)) => {
                        this.not_a_repo = true;
                        this.repo = None;
                    }
                    Err(error) => cx.emit(GitEvent::Error(error_text(&error))),
                }
                cx.notify();
            })
            .ok();
        }));
    }

    fn file(&self, path: &str) -> Option<&FileStatus> {
        self.status.files.iter().find(|f| f.path == path)
    }

    /// Runs a repository operation, then refreshes.
    pub fn run_op(&mut self, op: RepoOp, label: &str, cx: &mut Context<Self>) {
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        self.busy = Some(label.to_string());
        let is_commit = matches!(op, RepoOp::Commit { .. });
        let work = GitService::run(cx, move |_| async move {
            let context = OpContext {
                identity: None,
                net: None,
                cancel: None,
            };
            repo.execute(&op, context).await
        });
        self.op_task = Some(cx.spawn(async move |this, cx| {
            let result = work.await;
            this.update(cx, |this, cx| {
                this.busy = None;
                match result {
                    Some(Ok(summary)) => {
                        if is_commit {
                            this.message.update(cx, |m, cx| m.set_text("", cx));
                        }
                        if let Some(summary) = summary.filter(|s| !s.trim().is_empty()) {
                            cx.emit(GitEvent::Notice(summary));
                        }
                    }
                    Some(Err(error)) => cx.emit(GitEvent::Error(error_text(&error))),
                    None => {}
                }
                this.refresh(false, cx);
                cx.notify();
            })
            .ok();
        }));
        cx.notify();
    }

    pub fn stage(&mut self, path: &str, cx: &mut Context<Self>) {
        self.run_op(RepoOp::Stage(vec![path.to_string()]), "Staging…", cx);
    }

    pub fn unstage(&mut self, path: &str, cx: &mut Context<Self>) {
        self.run_op(RepoOp::Unstage(vec![path.to_string()]), "Unstaging…", cx);
    }

    pub fn discard(&mut self, path: &str, cx: &mut Context<Self>) {
        self.run_op(RepoOp::Discard(vec![path.to_string()]), "Discarding…", cx);
    }

    pub fn stage_all(&mut self, cx: &mut Context<Self>) {
        self.run_op(RepoOp::StageAll, "Staging…", cx);
    }

    pub fn unstage_all(&mut self, cx: &mut Context<Self>) {
        self.run_op(RepoOp::UnstageAll, "Unstaging…", cx);
    }

    pub fn fetch(&mut self, cx: &mut Context<Self>) {
        self.run_op(
            RepoOp::Fetch {
                remote: None,
                prune: true,
            },
            "Fetching…",
            cx,
        );
    }

    pub fn pull(&mut self, cx: &mut Context<Self>) {
        self.run_op(RepoOp::Pull(PullMode::FastForwardOnly), "Pulling…", cx);
    }

    pub fn push(&mut self, cx: &mut Context<Self>) {
        self.run_op(
            RepoOp::Push {
                remote: Some("origin".to_string()),
                force_with_lease: false,
            },
            "Pushing…",
            cx,
        );
    }

    /// Commits the staged changes, or every change when nothing is staged.
    pub fn commit(&mut self, cx: &mut Context<Self>) {
        let message = self.message.read(cx).text().trim().to_string();
        if message.is_empty() {
            cx.emit(GitEvent::Error("Write a commit message first".to_string()));
            return;
        }
        if self.status.files.is_empty() {
            cx.emit(GitEvent::Error("Nothing to commit".to_string()));
            return;
        }
        let Some(repo) = self.repo.clone() else {
            return;
        };
        if self.busy.is_some() {
            return;
        }
        if self.status.staged().is_empty() {
            // Stage everything first, as one step with the commit.
            self.busy = Some("Committing…".to_string());
            let work = GitService::run(cx, move |_| async move {
                let context = || OpContext {
                    identity: None,
                    net: None,
                    cancel: None,
                };
                repo.execute(&RepoOp::StageAll, context()).await?;
                repo.execute(
                    &RepoOp::Commit {
                        message,
                        amend: false,
                    },
                    context(),
                )
                .await
            });
            self.op_task = Some(cx.spawn(async move |this, cx| {
                let result = work.await;
                this.update(cx, |this, cx| {
                    this.busy = None;
                    match result {
                        Some(Ok(_)) => {
                            this.message.update(cx, |m, cx| m.set_text("", cx));
                            cx.emit(GitEvent::Notice("Committed all changes".to_string()));
                        }
                        Some(Err(error)) => cx.emit(GitEvent::Error(error_text(&error))),
                        None => {}
                    }
                    this.refresh(false, cx);
                })
                .ok();
            }));
            cx.notify();
        } else {
            self.run_op(
                RepoOp::Commit {
                    message,
                    amend: false,
                },
                "Committing…",
                cx,
            );
        }
    }

    /// Shows a file's staged or working-tree diff.
    pub fn select_file(&mut self, path: &str, staged: bool, cx: &mut Context<Self>) {
        self.selection = Some(GitSelection::File {
            path: path.to_string(),
            staged,
        });
        self.commit = None;
        self.load_diff(cx);
    }

    pub fn select_commit(&mut self, oid: Oid, cx: &mut Context<Self>) {
        self.selection = Some(GitSelection::Commit(oid));
        self.load_diff(cx);
    }

    /// After a refresh: drop a file selection that has no changes left.
    fn reload_selection(&mut self, cx: &mut Context<Self>) {
        if let Some(GitSelection::File { path, staged }) = &self.selection {
            let still = self.file(path).is_some_and(|f| {
                if *staged {
                    f.is_staged()
                } else {
                    f.is_unstaged()
                }
            });
            if !still {
                self.selection = None;
                self.diff = Arc::default();
                return;
            }
        }
        if matches!(self.selection, Some(GitSelection::File { .. })) {
            self.load_diff(cx);
        }
    }

    fn load_diff(&mut self, cx: &mut Context<Self>) {
        let (Some(repo), Some(selection)) = (self.repo.clone(), self.selection.clone()) else {
            return;
        };
        let untracked = match &selection {
            GitSelection::File { path, .. } => self
                .file(path)
                .is_some_and(|f| f.worktree == ChangeKind::Untracked),
            GitSelection::Commit(_) => false,
        };
        let orig = match &selection {
            GitSelection::File { path, .. } => self.file(path).and_then(|f| f.orig_path.clone()),
            GitSelection::Commit(_) => None,
        };
        let work = GitService::run(cx, move |_| async move {
            match selection {
                GitSelection::File { path, staged } => {
                    let diff = if untracked {
                        repo.diff_untracked(&path).await
                    } else if staged {
                        repo.diff_staged(&path, orig.as_deref()).await
                    } else {
                        repo.diff_unstaged(&path).await
                    }?;
                    Ok::<_, GitError>((diff.into_iter().collect::<Vec<FileDiff>>(), None))
                }
                GitSelection::Commit(oid) => {
                    let details = repo.commit_details(oid.as_str()).await?;
                    Ok((details.files.clone(), Some(details)))
                }
            }
        });
        self.diff_task = Some(cx.spawn(async move |this, cx| {
            let Some(result) = work.await else { return };
            this.update(cx, |this, cx| {
                match result {
                    Ok((files, details)) => {
                        this.diff = Arc::new(diff::rows(&files));
                        this.commit = details;
                    }
                    Err(error) => cx.emit(GitEvent::Error(error_text(&error))),
                }
                cx.notify();
            })
            .ok();
        }));
    }
}

/// Starts FerrumGIT on `dir` if it is installed.
pub fn open_in_ferrumgit(dir: &Path) -> Result<(), String> {
    let mut candidates: Vec<PathBuf> = Vec::new();
    if let Some(on_path) = crate::workspace::find_on_path("ferrumgit") {
        candidates.push(on_path);
    }
    if let Some(home) = std::env::var_os("HOME").map(PathBuf::from) {
        candidates.push(home.join("Applications/Ferrum/FerrumGIT.app/Contents/MacOS/ferrumgit"));
        candidates.push(home.join("Applications/FerrumGIT.app/Contents/MacOS/ferrumgit"));
    }
    candidates.push(PathBuf::from(
        "/Applications/FerrumGIT.app/Contents/MacOS/ferrumgit",
    ));
    let program = candidates
        .into_iter()
        .find(|p| p.is_file())
        .ok_or_else(|| {
            "FerrumGIT is not installed (install it from Ferrum Launcher)".to_string()
        })?;
    std::process::Command::new(program)
        .arg(dir)
        .current_dir(dir)
        .spawn()
        .map(|_| ())
        .map_err(|e| format!("Could not open FerrumGIT: {e}"))
}

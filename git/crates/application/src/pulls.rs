//! Pull / merge requests: forge detection from remotes, tokens (OS keychain
//! or the `gh` CLI), REST calls, and the git side (PR refs, diffs, checkout).

use crate::{AppError, AppResult, AppServices};
use ferrumgit_credentials::CredentialStore;
use ferrumgit_domain::{
    FileDiff, ForgeHost, ForgeKind, Profile, RepoEntry, RepoForge, RepoOp, Secret, TokenSource,
};
use ferrumgit_forge::auth::{TokenHelp, clean_token, gh_cli_token, token_help};
use ferrumgit_forge::checkout::{
    CheckoutPlan, checkout_plan, local_base_ref, local_head_ref, sanitize_branch,
};
use ferrumgit_forge::{
    Api, Forge, ForgeError, ForgeRepo, MergeMethod, NewLineComment, NewPullRequest, PrDetail,
    PrFilter, PrList, PullRequest, RateLimits, ReqwestTransport, ReviewEvent, Transport, client,
    resolve,
};
use ferrumgit_git::CancellationToken;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

/// Looks up the `gh` CLI token for a host (blocking).
pub type GhLookup = Arc<dyn Fn(&str) -> Option<Secret> + Send + Sync>;

/// Forge plumbing shared by all clones of [`AppServices`].
pub(crate) struct ForgeState {
    tokens: Arc<dyn CredentialStore>,
    transport: OnceLock<Result<Arc<dyn Transport>, String>>,
    gh: GhLookup,
    /// Rate-limit state per API base URL.
    limits: Mutex<HashMap<String, Arc<RateLimits>>>,
    /// Tokens read this session, by host (saves keychain prompts and `gh` runs).
    cache: Mutex<HashMap<String, Secret>>,
}

impl ForgeState {
    pub(crate) fn new(tokens: Arc<dyn CredentialStore>) -> Self {
        Self {
            tokens,
            transport: OnceLock::new(),
            gh: Arc::new(gh_cli_token),
            limits: Mutex::default(),
            cache: Mutex::default(),
        }
    }

    fn transport(&self) -> AppResult<Arc<dyn Transport>> {
        self.transport
            .get_or_init(|| {
                ReqwestTransport::new()
                    .map(|t| Arc::new(t) as Arc<dyn Transport>)
                    .map_err(|e| e.to_string())
            })
            .clone()
            .map_err(AppError::Invalid)
    }

    fn limits(&self, api_url: &str) -> Arc<RateLimits> {
        self.limits
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .entry(api_url.to_owned())
            .or_default()
            .clone()
    }

    fn cached(&self, host: &str) -> Option<Secret> {
        self.cache
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .get(host)
            .cloned()
    }

    fn remember(&self, host: &str, token: Option<Secret>) {
        let mut cache = self.cache.lock().unwrap_or_else(PoisonError::into_inner);
        match token {
            Some(t) => cache.insert(host.to_owned(), t),
            None => cache.remove(host),
        };
    }
}

/// What the pull requests panel needs to know before listing.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ForgeInfo {
    pub repo: ForgeRepo,
    pub token_source: TokenSource,
    /// A token is available from the selected source.
    pub has_token: bool,
    pub help: TokenHelp,
}

/// A pull request's changes, diffed locally against the merge base.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct PrChanges {
    pub files: Vec<FileDiff>,
    pub truncated: bool,
    pub merge_base: String,
}

fn blocking_err(e: tokio::task::JoinError) -> AppError {
    AppError::Credentials(e.to_string())
}

impl AppServices {
    /// Uses `tokens` for forge tokens instead of the OS keychain (tests).
    #[must_use]
    pub fn with_token_store(mut self, tokens: Arc<dyn CredentialStore>) -> Self {
        self.forge = Arc::new(ForgeState::new(tokens));
        self
    }

    /// Sends forge API requests through `transport` (tests replay fixtures)
    /// and answers `gh` token lookups with `gh` (tests never run the real CLI).
    #[must_use]
    pub fn with_forge_transport(mut self, transport: Arc<dyn Transport>, gh: GhLookup) -> Self {
        let state = ForgeState {
            tokens: self.forge.tokens.clone(),
            transport: OnceLock::from(Ok(transport)),
            gh,
            limits: Mutex::default(),
            cache: Mutex::default(),
        };
        self.forge = Arc::new(state);
        self
    }

    /// The forge project for `entry`, from its remotes and overrides. URLs
    /// rewritten by `url.<base>.insteadOf` are tried as configured too.
    pub async fn forge_repo(&self, entry: &RepoEntry) -> AppResult<ForgeRepo> {
        let repo = self.repo(&entry.path).await?;
        let pairs: Vec<(String, String)> = repo
            .remotes()
            .await?
            .into_iter()
            .map(|r| (r.name, r.fetch_url))
            .collect();
        match resolve(&pairs, &entry.forge) {
            Err(ForgeError::NotDetected) => {
                let configured = repo.configured_remote_urls().await?;
                Ok(resolve(&configured, &entry.forge)?)
            }
            other => Ok(other?),
        }
    }

    async fn token_source(&self, host: &str, kind: ForgeKind) -> AppResult<TokenSource> {
        if kind != ForgeKind::GitHub {
            return Ok(TokenSource::Keychain);
        }
        Ok(self
            .store
            .load_forge_hosts()
            .await?
            .into_iter()
            .find(|h| h.host == host)
            .map(|h| h.token_source)
            .unwrap_or_default())
    }

    async fn set_token_source(&self, host: &str, source: TokenSource) -> AppResult<()> {
        let mut hosts = self.store.load_forge_hosts().await?;
        hosts.retain(|h| h.host != host);
        if source != TokenSource::default() {
            hosts.push(ForgeHost {
                host: host.to_owned(),
                token_source: source,
            });
        }
        self.store.save_forge_hosts(&hosts).await?;
        Ok(())
    }

    /// The token for `repo`'s host from its configured source, if any.
    async fn forge_token(&self, repo: &ForgeRepo) -> AppResult<Option<Secret>> {
        if let Some(t) = self.forge.cached(&repo.host) {
            return Ok(Some(t));
        }
        let source = self.token_source(&repo.host, repo.kind).await?;
        let host = repo.host.clone();
        let state = self.forge.clone();
        let token = tokio::task::spawn_blocking(move || match source {
            TokenSource::GhCli => Ok((state.gh)(&host)),
            TokenSource::Keychain => state
                .tokens
                .get(&host)
                .map(|t| t.filter(|t| !t.trim().is_empty()).map(Secret::new)),
        })
        .await
        .map_err(blocking_err)?
        .map_err(|e| AppError::Credentials(e.to_string()))?;
        if let Some(t) = &token {
            self.forge.remember(&repo.host, Some(t.clone()));
        }
        Ok(token)
    }

    /// Forge, token source and onboarding text for `entry`.
    pub async fn forge_info(&self, entry: &RepoEntry) -> AppResult<ForgeInfo> {
        let repo = self.forge_repo(entry).await?;
        let token_source = self.token_source(&repo.host, repo.kind).await?;
        let has_token = self.forge_token(&repo).await?.is_some();
        Ok(ForgeInfo {
            help: token_help(&repo),
            repo,
            token_source,
            has_token,
        })
    }

    /// Saves a personal access token for `host` in the OS keychain and uses it.
    pub async fn set_forge_token(&self, host: &str, token: &str) -> AppResult<()> {
        let token = clean_token(token).map_err(|m| AppError::Invalid(m.to_owned()))?;
        let (state, account) = (self.forge.clone(), host.to_owned());
        let value = token.clone();
        tokio::task::spawn_blocking(move || state.tokens.set(&account, &value))
            .await
            .map_err(blocking_err)?
            .map_err(|e| AppError::Credentials(e.to_string()))?;
        self.set_token_source(host, TokenSource::Keychain).await?;
        self.forge.remember(host, Some(Secret::new(token)));
        tracing::info!(host, "forge token saved to the keychain");
        Ok(())
    }

    /// Uses `gh auth token --hostname <host>` for `host` (checked now, read on demand).
    pub async fn use_gh_cli(&self, host: &str) -> AppResult<()> {
        let (gh, h) = (self.forge.gh.clone(), host.to_owned());
        let token = tokio::task::spawn_blocking(move || gh(&h))
            .await
            .map_err(blocking_err)?;
        let Some(token) = token else {
            return Err(AppError::Invalid(format!(
                "The GitHub CLI has no login for {host}. Run `gh auth login --hostname {host}` in a terminal, or paste a token instead."
            )));
        };
        self.set_token_source(host, TokenSource::GhCli).await?;
        self.forge.remember(host, Some(token));
        Ok(())
    }

    /// Removes the stored token for `host` and stops using the `gh` CLI for it.
    pub async fn clear_forge_token(&self, host: &str) -> AppResult<()> {
        let (state, account) = (self.forge.clone(), host.to_owned());
        tokio::task::spawn_blocking(move || state.tokens.delete(&account))
            .await
            .map_err(blocking_err)?
            .map_err(|e| AppError::Credentials(e.to_string()))?;
        self.set_token_source(host, TokenSource::Keychain).await?;
        self.forge.remember(host, None);
        Ok(())
    }

    /// Saves per-repository forge overrides (remote, forge type, API URL).
    pub async fn set_repo_forge(
        &self,
        mut entry: RepoEntry,
        forge: RepoForge,
    ) -> AppResult<RepoEntry> {
        entry.forge = RepoForge {
            remote: forge.remote.trim().to_owned(),
            kind: forge.kind,
            api_url: forge.api_url.trim().trim_end_matches('/').to_owned(),
        };
        if !entry.forge.api_url.is_empty()
            && !entry.forge.api_url.starts_with("https://")
            && !entry.forge.api_url.starts_with("http://")
        {
            return Err(AppError::Invalid(
                "The API URL must start with https:// (or http://).".into(),
            ));
        }
        self.store.save_repo(&entry).await?;
        Ok(entry)
    }

    async fn forge_client(&self, entry: &RepoEntry) -> AppResult<Box<dyn Forge>> {
        let repo = self.forge_repo(entry).await?;
        let Some(token) = self.forge_token(&repo).await? else {
            return Err(ForgeError::NoToken {
                host: repo.host.clone(),
                hint: ferrumgit_forge::scope_hint(repo.kind).to_owned(),
            }
            .into());
        };
        let api = Api::new(
            self.forge.transport()?,
            repo.kind,
            &repo.host,
            &repo.api_url,
            token,
            self.forge.limits(&repo.api_url),
        );
        Ok(client(repo, api))
    }

    /// A rejected token is forgotten so a new one is read next time.
    fn checked<T>(&self, forge: &dyn Forge, result: Result<T, ForgeError>) -> AppResult<T> {
        if let Err(ForgeError::Unauthorized { .. }) = &result {
            self.forge.remember(&forge.repo().host, None);
        }
        Ok(result?)
    }

    pub async fn pull_requests(&self, entry: &RepoEntry, filter: PrFilter) -> AppResult<PrList> {
        {
            let f = self.forge_client(entry).await?;
            let r = f.list(filter).await;
            self.checked(f.as_ref(), r)
        }
    }

    pub async fn pull_request(&self, entry: &RepoEntry, number: u64) -> AppResult<PrDetail> {
        {
            let f = self.forge_client(entry).await?;
            let r = f.detail(number).await;
            self.checked(f.as_ref(), r)
        }
    }

    pub async fn comment_pull_request(
        &self,
        entry: &RepoEntry,
        number: u64,
        body: &str,
    ) -> AppResult<()> {
        {
            let f = self.forge_client(entry).await?;
            let r = f.comment(number, body).await;
            self.checked(f.as_ref(), r)
        }
    }

    pub async fn review_pull_request(
        &self,
        entry: &RepoEntry,
        number: u64,
        event: ReviewEvent,
        body: &str,
    ) -> AppResult<()> {
        {
            let f = self.forge_client(entry).await?;
            let r = f.review(number, event, body).await;
            self.checked(f.as_ref(), r)
        }
    }

    /// Starts a review thread on a diff line of `pr`.
    pub async fn comment_pull_request_line(
        &self,
        entry: &RepoEntry,
        pr: &PrDetail,
        comment: &NewLineComment,
    ) -> AppResult<()> {
        let f = self.forge_client(entry).await?;
        let r = f.line_comment(pr, comment).await;
        self.checked(f.as_ref(), r)
    }

    /// Replies in a review thread.
    pub async fn reply_pull_request_thread(
        &self,
        entry: &RepoEntry,
        number: u64,
        thread: &str,
        body: &str,
    ) -> AppResult<()> {
        let f = self.forge_client(entry).await?;
        let r = f.reply(number, thread, body).await;
        self.checked(f.as_ref(), r)
    }

    /// Merges on the forge if the head is still `head_sha`.
    pub async fn merge_pull_request(
        &self,
        entry: &RepoEntry,
        number: u64,
        method: MergeMethod,
        head_sha: &str,
    ) -> AppResult<()> {
        {
            let f = self.forge_client(entry).await?;
            let r = f.merge(number, method, head_sha).await;
            self.checked(f.as_ref(), r)
        }
    }

    /// Fetches the PR head and base into `refs/ferrumgit/pr/<n>/…` and diffs
    /// the head against its merge base with the base branch.
    pub async fn pull_request_changes(
        &self,
        entry: &RepoEntry,
        profile: Option<&Profile>,
        pr: &PullRequest,
        cancel: Option<CancellationToken>,
    ) -> AppResult<PrChanges> {
        let fr = self.forge_repo(entry).await?;
        if sanitize_branch(&pr.base_branch) != pr.base_branch || pr.base_branch.is_empty() {
            return Err(AppError::Invalid(format!(
                "unsupported base branch name {:?}",
                pr.base_branch
            )));
        }
        let (head, base) = (local_head_ref(pr.number), local_base_ref(pr.number));
        let specs = [
            format!("+{}:{head}", fr.head_ref(pr.number)),
            format!("+refs/heads/{}:{base}", pr.base_branch),
        ];
        self.fetch_pr_refs(entry, profile, &fr.remote, &specs, cancel)
            .await?;
        let repo = self.repo(&entry.path).await?;
        let merge_base = repo.merge_base(&base, &head).await?;
        let (files, truncated) = repo.diff_range(&merge_base, &head).await?;
        Ok(PrChanges {
            files,
            truncated,
            merge_base,
        })
    }

    async fn fetch_pr_refs(
        &self,
        entry: &RepoEntry,
        profile: Option<&Profile>,
        remote: &str,
        specs: &[String],
        cancel: Option<CancellationToken>,
    ) -> AppResult<()> {
        let net = self.net_env(profile).await?;
        let repo = self.repo(&entry.path).await?;
        repo.fetch_refspecs(remote, specs, Some(net), cancel)
            .await
            .map_err(|e| e.with_ssh_hint().into())
    }

    /// Checks out a pull request: same-project branches are tracked under
    /// their own name, forks get `pr-<n>-<branch>` at the PR head. An
    /// existing local branch is fast-forwarded when possible. Returns a summary.
    pub async fn checkout_pull_request(
        &self,
        entry: &RepoEntry,
        profile: Option<&Profile>,
        pr: &PullRequest,
        cancel: Option<CancellationToken>,
    ) -> AppResult<String> {
        let fr = self.forge_repo(entry).await?;
        let repo = self.repo(&entry.path).await?;
        let locals: Vec<String> = repo
            .refs()
            .await?
            .into_iter()
            .filter(|r| r.is(ferrumgit_domain::RefKind::LocalBranch))
            .map(|r| r.name)
            .collect();
        let plan = checkout_plan(pr, &fr.remote, &locals);
        let head = local_head_ref(pr.number);
        let mut specs = vec![format!("+{}:{head}", fr.head_ref(pr.number))];
        if let CheckoutPlan::Track { local, .. } = &plan {
            specs.push(format!(
                "+refs/heads/{local}:refs/remotes/{}/{local}",
                fr.remote
            ));
        }
        self.fetch_pr_refs(entry, profile, &fr.remote, &specs, cancel)
            .await?;
        let (op, ff_from) = match &plan {
            CheckoutPlan::Track {
                remote_branch,
                local,
                exists: false,
            } => (
                RepoOp::CheckoutRemote {
                    remote_branch: remote_branch.clone(),
                    local: local.clone(),
                },
                None,
            ),
            CheckoutPlan::Track {
                remote_branch,
                local,
                exists: true,
            } => (RepoOp::Checkout(local.clone()), Some(remote_branch.clone())),
            CheckoutPlan::Fork {
                local,
                exists: false,
            } => (
                RepoOp::CreateBranch {
                    name: local.clone(),
                    start: Some(head.clone()),
                    checkout: true,
                },
                None,
            ),
            CheckoutPlan::Fork {
                local,
                exists: true,
            } => (RepoOp::Checkout(local.clone()), Some(head.clone())),
        };
        self.run_op(entry, profile, &op, None).await?;
        let local = plan.local().to_owned();
        if let Some(rev) = ff_from
            && let Err(e) = repo.merge_ff_only(&rev).await
        {
            tracing::debug!(error = %e, "PR branch could not be fast-forwarded");
            return Ok(format!(
                "Checked out {local}; it has diverged from the pull request, so it was not updated."
            ));
        }
        Ok(format!("Checked out {local}"))
    }

    /// Creates a pull / merge request from the current branch, pushing it
    /// first when it has no upstream or unpushed commits.
    pub async fn create_pull_request(
        &self,
        entry: &RepoEntry,
        profile: Option<&Profile>,
        mut new: NewPullRequest,
        cancel: Option<CancellationToken>,
    ) -> AppResult<PullRequest> {
        let fr = self.forge_repo(entry).await?;
        let status = self.repo(&entry.path).await?.status().await?;
        let Some(branch) = status.branch.head.clone() else {
            return Err(AppError::Invalid(
                "HEAD is detached; check out the branch to propose first.".into(),
            ));
        };
        new.head = branch;
        // Validate before pushing anything.
        if new.title.trim().is_empty() {
            return Err(AppError::Invalid("Enter a title.".into()));
        }
        if new.base.trim().is_empty() || new.base == new.head {
            return Err(AppError::Invalid(
                "Choose a base branch other than the current one.".into(),
            ));
        }
        // A token is needed anyway; fail before pushing if there is none.
        self.forge_client(entry).await?;
        if status.branch.upstream.is_none() || status.branch.ahead > 0 {
            let remote = status.branch.upstream.is_none().then(|| fr.remote.clone());
            self.run_op(
                entry,
                profile,
                &RepoOp::Push {
                    remote,
                    force_with_lease: false,
                },
                cancel,
            )
            .await?;
        }
        {
            let f = self.forge_client(entry).await?;
            let r = f.create(&new).await;
            self.checked(f.as_ref(), r)
        }
    }
}

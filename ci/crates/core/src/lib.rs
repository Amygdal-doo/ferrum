//! CI core behind FerrumIDE's CI page: GitHub Actions for a
//! repository, built on FerrumGIT's forge layer (`ferrumgit-forge`) so auth
//! headers, rate limits, error messages and the testable [`Transport`] are
//! shared.
//!
//! - [`repo`]: project folder → GitHub repository (from `.git/config`).
//! - [`auth`]: token lookup (FerrumGIT keychain → `gh` → env), never shown.
//! - [`actions`]: runs, jobs, logs, re-run / cancel / dispatch endpoints.
//! - [`cache`]: `ETag` / `If-None-Match` so polling is cheap.
//! - [`logs`] + [`ansi`]: job log parsing (groups, errors, colours, search).
//! - [`dispatch`]: `workflow_dispatch` inputs from the workflow file.
//! - [`watch`]: poll intervals and "your run finished" detection.

pub mod actions;
pub mod ansi;
pub mod auth;
pub mod cache;
pub mod dispatch;
pub mod format;
pub mod logs;
pub mod model;
pub mod repo;
pub mod watch;

pub use actions::{Actions, EVENTS, RunFilter, StatusFilter};
pub use ferrumgit_forge::{
    Api, CheckState, ForgeError, ForgeRepo, ForgeResult, PullRequest, RateLimits, RateState,
    Transport,
};
pub use model::{Job, RunState, RunsPage, Step, Workflow, WorkflowRun};

use ferrumgit_domain::Secret;
use ferrumgit_forge::PrFilter;
use std::sync::Arc;

/// Runs per page in the list.
pub const PAGE_SIZE: u32 = 30;

/// Everything the CI page talks to for one repository.
#[derive(Debug, Clone)]
pub struct Client {
    pub repo: ForgeRepo,
    pub actions: Actions,
}

impl Client {
    /// `transport` should already be wrapped in [`cache::EtagTransport`].
    #[must_use]
    pub fn new(
        repo: ForgeRepo,
        transport: Arc<dyn Transport>,
        token: Secret,
        limits: Arc<RateLimits>,
    ) -> Self {
        let api = Api::new(
            transport,
            repo.kind,
            &repo.host,
            &repo.api_url,
            token,
            limits,
        );
        Self {
            actions: Actions::new(&repo, api),
            repo,
        }
    }

    /// Open pull requests with their combined check status (FerrumGIT's PR listing).
    pub async fn pull_requests(&self) -> ForgeResult<Vec<PullRequest>> {
        let forge = ferrumgit_forge::client(self.repo.clone(), self.actions.api().clone());
        Ok(forge.list(PrFilter::All).await?.prs)
    }

    /// The token owner's login; also checks the token works.
    pub async fn viewer(&self) -> ForgeResult<String> {
        let forge = ferrumgit_forge::client(self.repo.clone(), self.actions.api().clone());
        forge.viewer().await
    }
}

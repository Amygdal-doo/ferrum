//! GitHub and GitLab pull / merge requests for FerrumGIT.
//!
//! [`remote`] maps a repository's remotes to a forge project; [`client`]
//! returns a [`Forge`] for it (GitHub REST v3 or GitLab v4) that sends
//! requests through a [`Transport`] — reqwest in the app, recorded JSON
//! fixtures in tests. [`Api`] adds auth headers, tracks rate limits and turns
//! HTTP errors into clear messages. Tokens are [`Secret`]s and never logged.
//!
//! [`Secret`]: ferrumgit_domain::Secret

pub mod api;
pub mod auth;
pub mod checkout;
mod client;
mod error;
pub mod github;
pub mod gitlab;
pub mod markdown;
pub mod model;
pub mod remote;
pub mod transport;

pub use api::{Api, RateLimits, RateState};
pub use client::{Forge, PrList, STATUS_LOOKUPS, client};
pub use error::{ForgeError, ForgeResult, scope_hint};
pub use model::{
    Check, CheckState, MergeMethod, NewPullRequest, PrCommit, PrDetail, PrFilter, PrState,
    PullRequest, ReviewDecision, ReviewEvent, ReviewState, TimelineItem, TimelineKind,
};
pub use remote::{ForgeRepo, resolve};
pub use transport::{HttpRequest, HttpResponse, Method, ReqwestTransport, Transport};

//! HTTP execution for FerrumAPI: variable substitution, auth, bodies,
//! settings and cancellation on top of `reqwest` (rustls backend).
//!
//! Nothing in this crate logs URLs, header values, bodies or credentials.

mod auth;
mod body;
mod build;
mod client;
pub mod cookies;
mod errors;
mod response;
mod substitute;

pub use tokio_util::sync::CancellationToken;

pub use build::parse_url;
pub use client::HttpClient;
pub use cookies::{parse_set_cookie, parse_set_cookies};
pub use substitute::{substitute, unresolved_variables};

//! `POST /api/ping`: FerrumLauncher's anonymous daily count (see
//! `crate::usage`). Only the validated payload is stored; the request's IP
//! address and user agent are never read.
//!
//! 204 accepted (or no database configured), 400 invalid, 413 too large,
//! 503 when the database can't be written (the launcher retries later).

use axum::{
    body::Bytes,
    extract::State,
    http::{StatusCode, header},
    response::{IntoResponse, Response},
};

use super::AppState;
use crate::usage;

pub const PATH: &str = "/api/ping";

pub async fn ping(State(s): State<AppState>, body: Bytes) -> Response {
    let status = match usage::parse_ping(&body) {
        Err(e) if e == usage::TOO_LARGE => StatusCode::PAYLOAD_TOO_LARGE,
        Err(_) => StatusCode::BAD_REQUEST,
        Ok(ping) => match s.usage.record(&ping).await {
            Ok(_) => StatusCode::NO_CONTENT,
            Err(err) => {
                tracing::warn!(%err, "could not store a usage ping");
                StatusCode::SERVICE_UNAVAILABLE
            }
        },
    };
    (status, [(header::CACHE_CONTROL, "no-store")]).into_response()
}

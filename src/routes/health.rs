//! Liveness probe used by Railway (`healthcheckPath = "/healthz"`).

use axum::{http::header, response::IntoResponse};

pub async fn healthz() -> impl IntoResponse {
    (
        [
            (header::CONTENT_TYPE, "text/plain; charset=utf-8"),
            (header::CACHE_CONTROL, "no-store"),
        ],
        "ok",
    )
}

//! Cross-cutting HTTP middleware: compression, tracing, security headers and
//! a default cache policy.

use axum::{
    Router,
    http::{HeaderName, HeaderValue, header},
};
use tower_http::{
    compression::CompressionLayer,
    set_header::SetResponseHeaderLayer,
    trace::{DefaultOnResponse, TraceLayer},
};
use tracing::Level;

/// Default `Cache-Control` for responses that do not set their own
/// (HTML pages, robots.txt, sitemap.xml). Short, so content edits go live fast.
pub const DEFAULT_CACHE: &str = "public, max-age=300";

/// Security headers added to every response (unless a handler set them).
/// No CSP on purpose: it would need to allow Google Fonts and must be tuned
/// together with the page templates.
const SECURITY_HEADERS: [(HeaderName, &str); 3] = [
    (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    (header::REFERRER_POLICY, "strict-origin-when-cross-origin"),
    (header::X_FRAME_OPTIONS, "DENY"),
];

fn if_not_present(name: HeaderName, value: &'static str) -> SetResponseHeaderLayer<HeaderValue> {
    SetResponseHeaderLayer::if_not_present(name, HeaderValue::from_static(value))
}

/// Wrap the router with security headers, default caching, compression and
/// request tracing.
pub fn with_middleware(router: Router) -> Router {
    let mut router = router.layer(if_not_present(header::CACHE_CONTROL, DEFAULT_CACHE));
    for (name, value) in SECURITY_HEADERS {
        router = router.layer(if_not_present(name, value));
    }
    router.layer(CompressionLayer::new()).layer(
        TraceLayer::new_for_http()
            // Path only: query strings stay out of the logs.
            .make_span_with(|req: &axum::http::Request<_>| {
                tracing::info_span!("request", method = %req.method(), path = %req.uri().path())
            })
            .on_response(DefaultOnResponse::new().level(Level::INFO)),
    )
}

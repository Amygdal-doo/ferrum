//! Cross-cutting HTTP middleware: compression, tracing, security headers,
//! a default cache policy, the `www.` → canonical host redirect, the htmx
//! escape for links that are not site pages, and the page-view count.

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

/// Where a request for `www.<site host>` should go: the same path and query on
/// `site_url`. `None` for any other host, so health checks and the platform's
/// own domain are served as usual.
pub fn www_redirect(site_url: &str, host: &str, path_and_query: &str) -> Option<String> {
    let site_host = site_url.split_once("://")?.1.split('/').next()?;
    let host = host.split(':').next()?;
    (host.strip_prefix("www.")? == site_host).then(|| format!("{site_url}{path_and_query}"))
}

/// Permanently redirects `www.<site host>` to the canonical `SITE_URL`.
pub async fn canonical_host(
    axum::extract::State(site_url): axum::extract::State<std::sync::Arc<str>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    let host = req
        .headers()
        .get(header::HOST)
        .and_then(|h| h.to_str().ok())
        .unwrap_or_default();
    let pq = req.uri().path_and_query().map_or("/", |pq| pq.as_str());
    match www_redirect(&site_url, host, pq) {
        Some(to) => axum::response::Redirect::permanent(&to).into_response(),
        None => next.run(req).await,
    }
}

/// htmx request headers. `HX-Boosted` marks a link followed with `hx-boost`.
const HX_REQUEST: HeaderName = HeaderName::from_static("hx-request");
const HX_BOOSTED: HeaderName = HeaderName::from_static("hx-boosted");
const HX_REDIRECT: HeaderName = HeaderName::from_static("hx-redirect");

/// Whether a boosted link to `path` may be loaded by htmx: a page of this
/// site. Downloads, the feed, files and the accounts service must load in
/// full, so the browser handles them itself.
pub fn boostable(path: &str) -> bool {
    const FULL_LOAD: [&str; 5] = [
        crate::site::MACOS_DOWNLOAD_PATH,
        crate::updates::FEED_PATH,
        "/sitemap.xml",
        "/robots.txt",
        "/favicon.ico",
    ];
    crate::visits::countable(path) && !FULL_LOAD.contains(&path)
}

/// Answers a boosted request for anything but a site page with `HX-Redirect`,
/// which makes htmx load the same URL as a normal page instead. Decided
/// before the handler runs, so e.g. `/logout` is never sent twice.
pub async fn full_page_escape(
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use axum::response::IntoResponse;
    if req.headers().contains_key(HX_BOOSTED) && !boostable(req.uri().path()) {
        let to = req
            .uri()
            .path_and_query()
            .map_or("/", |pq| pq.as_str())
            .to_owned();
        return [(HX_REDIRECT, to)].into_response();
    }
    next.run(req).await
}

/// Counts a page view (`crate::visits`): a `GET` from a browser that is
/// answered with an HTML page. Not prefetches, bots, assets or the API, and
/// not htmx's own requests (the `/stats` refresh); a boosted link is a view.
pub async fn count_view(
    axum::extract::State(visits): axum::extract::State<std::sync::Arc<crate::visits::Visits>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    use crate::visits;
    // In a block: a borrow of the request (its body is not `Sync`) must not
    // be held across the `await` below.
    let view = {
        let headers = req.headers();
        let header = |name: HeaderName| headers.get(name).and_then(|h| h.to_str().ok());
        let prefetch = ["sec-purpose", "purpose"].into_iter().any(|name| {
            header(HeaderName::from_static(name)).is_some_and(|v| v.contains("prefetch"))
        });
        let path = req.uri().path();
        let htmx_partial = headers.contains_key(HX_REQUEST) && !headers.contains_key(HX_BOOSTED);
        (visits.enabled()
            && req.method() == axum::http::Method::GET
            && !prefetch
            && !htmx_partial
            && visits::countable(path)
            && visits::is_browser(header(header::USER_AGENT).unwrap_or_default()))
        .then(|| {
            let source = visits::source(header(header::REFERER), visits.site_host());
            (path.to_owned(), source)
        })
    };
    let res = next.run(req).await;
    if let Some((path, source)) = view {
        let html = res
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.starts_with("text/html"));
        if res.status() == axum::http::StatusCode::OK && html {
            visits.add(&path, source);
        }
    }
    res
}

#[cfg(test)]
mod tests {
    use super::{boostable, www_redirect};

    #[test]
    fn only_site_pages_are_boostable() {
        assert!(boostable("/"));
        assert!(boostable("/ferrumide"));
        assert!(boostable("/updates"));
        assert!(boostable("/does-not-exist"));
        assert!(!boostable(crate::site::MACOS_DOWNLOAD_PATH));
        assert!(!boostable(crate::updates::FEED_PATH));
        assert!(!boostable("/assets/img/og.png"));
        assert!(!boostable("/logout"));
        assert!(!boostable("/account/settings"));
        assert!(!boostable("/sitemap.xml"));
    }

    #[test]
    fn www_goes_to_the_canonical_host_with_path_and_query() {
        let site = "https://tryferrum.dev";
        assert_eq!(
            www_redirect(site, "www.tryferrum.dev", "/apps?x=1").as_deref(),
            Some("https://tryferrum.dev/apps?x=1")
        );
        assert_eq!(
            www_redirect(site, "www.tryferrum.dev:443", "/").as_deref(),
            Some("https://tryferrum.dev/")
        );
        assert_eq!(www_redirect(site, "tryferrum.dev", "/"), None);
        assert_eq!(www_redirect(site, "tryferrum.dev", "/"), None);
        assert_eq!(www_redirect(site, "www.example.com", "/"), None);
    }
}

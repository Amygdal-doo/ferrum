//! End-to-end HTTP tests against the full router (no network).
#![allow(clippy::unwrap_used)]

use axum::{
    body::Body,
    http::{HeaderMap, Method, Request, StatusCode, header},
};
use std::sync::Arc;

use http_body_util::BodyExt;
use tower::ServiceExt;
use web_ferrum::{
    Config, app,
    releases::MacosDownload,
    routes::{app_with, assets},
};

struct Res {
    status: StatusCode,
    headers: HeaderMap,
    body: Vec<u8>,
}

impl Res {
    fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
    fn header(&self, name: header::HeaderName) -> &str {
        self.headers
            .get(name)
            .map(|v| v.to_str().unwrap())
            .unwrap_or("")
    }
}

async fn send(req: Request<Body>) -> Res {
    let res = app(Config::default()).oneshot(req).await.unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Res {
        status,
        headers,
        body,
    }
}

const PAGES: [&str; 12] = [
    "/",
    "/features",
    "/ferrum-ide",
    "/ferrumapi",
    "/ferrumgit",
    "/ferrumdata",
    "/ferrumterm",
    "/ferrumkit",
    "/ferrumdock",
    "/ferrum-toolbox",
    "/download",
    "/about",
];

async fn get(uri: &str) -> Res {
    send(Request::get(uri).body(Body::empty()).unwrap()).await
}

#[tokio::test]
async fn pages_render_html_with_title() {
    for path in PAGES {
        let res = get(path).await;
        assert_eq!(res.status, StatusCode::OK, "{path}");
        assert!(
            res.header(header::CONTENT_TYPE).starts_with("text/html"),
            "{path}"
        );
        assert!(res.text().contains("<title>"), "{path}");
        assert_eq!(
            res.header(header::CACHE_CONTROL),
            "public, max-age=300",
            "{path}"
        );
    }
}

#[tokio::test]
async fn download_page_offers_public_macos_download() {
    let res = get("/download").await;
    assert_eq!(res.status, StatusCode::OK);
    let html = res.text();
    assert!(html.contains("Get Ferrum Toolbox"));
    assert!(html.contains(r#"href="/download/macos""#));
    assert!(html.contains("https://github.com/Amygdal-doo/ferrum-releases/releases"));
    assert!(!html.contains("gh auth login"));
}

fn fake_releases(result: Result<&'static str, &'static str>) -> MacosDownload {
    MacosDownload::new(Arc::new(move || {
        Box::pin(async move { result.map(str::to_owned).map_err(str::to_owned) })
    }))
}

async fn get_with(resolver: MacosDownload, uri: &str) -> Res {
    let req = Request::get(uri).body(Body::empty()).unwrap();
    let res = app_with(Config::default(), resolver)
        .oneshot(req)
        .await
        .unwrap();
    Res {
        status: res.status(),
        headers: res.headers().clone(),
        body: Vec::new(),
    }
}

#[tokio::test]
async fn macos_download_redirects_to_newest_dmg() {
    let json = r#"[
      {"tag_name": "toolbox-v0.2.0", "draft": false, "prerelease": false,
       "assets": [{"name": "FerrumToolbox-0.2.0-aarch64.dmg",
                   "browser_download_url": "https://dl.test/0.2.0.dmg"}]}
    ]"#;
    let res = get_with(fake_releases(Ok(json)), "/download/macos").await;
    assert_eq!(res.status, StatusCode::FOUND);
    assert_eq!(res.header(header::LOCATION), "https://dl.test/0.2.0.dmg");
    assert_eq!(res.header(header::CACHE_CONTROL), "no-store");
}

#[tokio::test]
async fn macos_download_falls_back_to_releases_page() {
    let res = get_with(fake_releases(Err("offline")), "/download/macos").await;
    assert_eq!(res.status, StatusCode::FOUND);
    assert_eq!(
        res.header(header::LOCATION),
        "https://github.com/Amygdal-doo/ferrum-releases/releases"
    );
}

#[tokio::test]
async fn brand_assets_are_served() {
    for path in [
        "/assets/img/brand/ferrum.svg",
        "/assets/img/brand/toolbox.svg",
        "/assets/img/brand/ferrum-wordmark.svg",
        "/assets/img/favicon.svg",
    ] {
        let res = get(path).await;
        assert_eq!(res.status, StatusCode::OK, "{path}");
        assert!(
            res.header(header::CONTENT_TYPE)
                .starts_with("image/svg+xml"),
            "{path}"
        );
    }
    for path in ["/assets/img/og.png", "/assets/img/apple-touch-icon.png"] {
        let res = get(path).await;
        assert_eq!(res.status, StatusCode::OK, "{path}");
        assert_eq!(res.header(header::CONTENT_TYPE), "image/png", "{path}");
    }
}

#[tokio::test]
async fn unknown_page_is_html_404() {
    let res = get("/does-not-exist").await;
    assert_eq!(res.status, StatusCode::NOT_FOUND);
    assert!(res.header(header::CONTENT_TYPE).starts_with("text/html"));
    assert_eq!(res.header(header::CACHE_CONTROL), "no-cache");
}

#[tokio::test]
async fn healthz_is_plain_ok() {
    let res = get("/healthz").await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.text(), "ok");
    assert!(res.header(header::CONTENT_TYPE).starts_with("text/plain"));
    assert_eq!(res.header(header::CACHE_CONTROL), "no-store");
}

#[tokio::test]
async fn head_is_supported() {
    let req = Request::builder()
        .method(Method::HEAD)
        .uri("/healthz")
        .body(Body::empty())
        .unwrap();
    let res = send(req).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.body.is_empty());
}

#[tokio::test]
async fn robots_points_to_sitemap() {
    let res = get("/robots.txt").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header(header::CONTENT_TYPE).starts_with("text/plain"));
    assert!(
        res.text()
            .contains("Sitemap: http://localhost:8080/sitemap.xml")
    );
}

#[tokio::test]
async fn sitemap_lists_all_pages() {
    let res = get("/sitemap.xml").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(
        res.header(header::CONTENT_TYPE)
            .starts_with("application/xml")
    );
    let xml = res.text();
    for path in PAGES {
        let loc = format!("<loc>http://localhost:8080{path}</loc>");
        assert!(xml.contains(&loc), "missing {loc}");
    }
    assert_eq!(xml.matches("<url>").count(), PAGES.len());
}

#[tokio::test]
async fn versioned_asset_is_immutable_css() {
    let url = assets::url("css/tokens.css");
    assert!(url.contains("?v="));
    let res = get(&url).await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header(header::CONTENT_TYPE).starts_with("text/css"));
    assert!(res.header(header::CACHE_CONTROL).contains("immutable"));
    assert!(
        res.header(header::CACHE_CONTROL)
            .contains("max-age=31536000")
    );
    assert!(!res.header(header::ETAG).is_empty());
}

#[tokio::test]
async fn unversioned_asset_revalidates() {
    let res = get("/assets/css/tokens.css").await;
    assert_eq!(res.status, StatusCode::OK);
    assert!(res.header(header::CONTENT_TYPE).starts_with("text/css"));
    assert!(!res.header(header::CACHE_CONTROL).contains("immutable"));
}

#[tokio::test]
async fn etag_round_trip_gives_304() {
    let first = get("/assets/css/tokens.css").await;
    let etag = first.header(header::ETAG).to_owned();
    assert!(etag.starts_with('"'));
    let req = Request::get("/assets/css/tokens.css")
        .header(header::IF_NONE_MATCH, &etag)
        .body(Body::empty())
        .unwrap();
    let res = send(req).await;
    assert_eq!(res.status, StatusCode::NOT_MODIFIED);
    assert!(res.body.is_empty());
    assert_eq!(res.header(header::ETAG), etag);

    let req = Request::get("/assets/css/tokens.css")
        .header(header::IF_NONE_MATCH, "\"stale\"")
        .body(Body::empty())
        .unwrap();
    assert_eq!(send(req).await.status, StatusCode::OK);
}

#[tokio::test]
async fn missing_asset_is_404() {
    assert_eq!(
        get("/assets/css/nope.css").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn gzip_when_accepted() {
    let req = Request::get("/")
        .header(header::ACCEPT_ENCODING, "gzip")
        .body(Body::empty())
        .unwrap();
    let res = send(req).await;
    assert_eq!(res.status, StatusCode::OK);
    assert_eq!(res.header(header::CONTENT_ENCODING), "gzip");
    // gzip magic number
    assert_eq!(res.body.get(..2), Some(&[0x1f, 0x8b][..]));
}

#[tokio::test]
async fn security_headers_on_every_response() {
    for path in ["/", "/healthz", "/assets/css/tokens.css", "/does-not-exist"] {
        let res = get(path).await;
        assert_eq!(
            res.header(header::X_CONTENT_TYPE_OPTIONS),
            "nosniff",
            "{path}"
        );
        assert_eq!(
            res.header(header::REFERRER_POLICY),
            "strict-origin-when-cross-origin",
            "{path}"
        );
        assert_eq!(res.header(header::X_FRAME_OPTIONS), "DENY", "{path}");
        assert!(res.headers.get(header::CONTENT_SECURITY_POLICY).is_none());
    }
}

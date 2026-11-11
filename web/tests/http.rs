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

const PAGES: [&str; 17] = [
    "/",
    "/features",
    "/ferrumapi",
    "/ferrumgit",
    "/ferrumdata",
    "/ferrumterm",
    "/ferrumkit",
    "/ferrumdock",
    "/ferrumports",
    "/ferrumenv",
    "/ferrum-editor",
    "/ferrum-launcher",
    "/download",
    "/updates",
    "/updates/ferrum-public-alpha",
    "/about",
    "/privacy",
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
    assert!(html.contains("Get Ferrum Launcher"));
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
      {"tag_name": "launcher-v0.2.0", "draft": false, "prerelease": false,
       "assets": [{"name": "FerrumLauncher-0.2.0-aarch64.dmg",
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
        "/assets/img/brand/launcher.svg",
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
async fn new_app_pages_show_their_logo_demo_and_status() {
    for (path, name, logo) in [
        ("/ferrumports", "FerrumPorts", "ports"),
        ("/ferrumenv", "FerrumEnv", "env"),
    ] {
        let html = get(path).await.text();
        assert!(html.contains(&format!(">{name}</h1>")), "{path}");
        assert!(
            html.contains(&format!("Animated preview of {name}")),
            "{path}"
        );
        assert!(
            html.contains(&format!("/assets/img/brand/{logo}.svg?v=")),
            "{path}"
        );
        assert!(html.contains("Works today"), "{path}");
        assert!(html.contains("Not built yet"), "{path}");
        let logo_res = get(&format!("/assets/img/brand/{logo}.svg")).await;
        assert_eq!(logo_res.status, StatusCode::OK, "{logo}");
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

#[tokio::test]
async fn old_toolbox_page_redirects_to_launcher() {
    let res = get("/ferrum-toolbox").await;
    assert_eq!(res.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(res.header(header::LOCATION), "/ferrum-launcher");
}

#[tokio::test]
async fn old_ide_page_redirects_to_editor() {
    let res = get("/ferrum-ide").await;
    assert_eq!(res.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(res.header(header::LOCATION), "/ferrum-editor");
}

#[tokio::test]
async fn old_ci_page_redirects_to_the_editors_ci_section() {
    let res = get("/ferrumci").await;
    assert_eq!(res.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(res.header(header::LOCATION), "/ferrum-editor#ci");
    let editor = get("/ferrum-editor").await.text();
    assert!(editor.contains(r#"id="ci""#));
    assert!(editor.contains("GitHub Actions"));
}

#[tokio::test]
async fn old_logs_page_redirects_to_the_editors_logs_section() {
    let res = get("/ferrumlogs").await;
    assert_eq!(res.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(res.header(header::LOCATION), "/ferrum-editor#logs");
    let editor = get("/ferrum-editor").await.text();
    assert!(editor.contains(r#"id="logs""#));
}

#[tokio::test]
async fn ferrumci_and_ferrumlogs_are_gone_from_the_site() {
    for path in [
        "/",
        "/features",
        "/download",
        "/ferrum-launcher",
        "/updates/ferrum-public-alpha",
    ] {
        let html = get(path).await.text();
        for gone in [
            "FerrumCI",
            "FerrumLogs",
            r#"href="/ferrumci""#,
            r#"href="/ferrumlogs""#,
        ] {
            assert!(!html.contains(gone), "{path}: {gone}");
        }
    }
    for logo in ["ci", "logs"] {
        assert_eq!(
            get(&format!("/assets/img/brand/{logo}.svg")).await.status,
            StatusCode::NOT_FOUND
        );
    }
    let sitemap = get("/sitemap.xml").await.text();
    assert!(!sitemap.contains("/ferrumci"));
    assert!(!sitemap.contains("/ferrumlogs"));
}

#[tokio::test]
async fn mailing_list_is_gone() {
    for (method, path) in [
        (Method::POST, "/api/subscribe"),
        (Method::GET, "/api/confirm?token=x"),
        (Method::GET, "/api/subscribers.csv"),
        (Method::GET, "/unsubscribe?token=x"),
        (Method::POST, "/unsubscribe"),
    ] {
        let req = Request::builder()
            .method(method.clone())
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let status = send(req).await.status;
        assert!(
            status == StatusCode::NOT_FOUND || status == StatusCode::METHOD_NOT_ALLOWED,
            "{method} {path}: {status}"
        );
    }
    for path in ["/", "/updates", "/updates/ferrum-public-alpha", "/privacy"] {
        let html = get(path).await.text();
        assert!(!html.contains("<form"), "{path}");
        assert!(!html.contains("mailing-list"), "{path}");
        assert!(!html.contains("Mailing list"), "{path}");
    }
    let robots = get("/robots.txt").await.text();
    assert!(!robots.contains("unsubscribe"));
}

#[tokio::test]
async fn updates_pages_point_to_the_feed() {
    let index = get("/updates").await;
    assert_eq!(index.status, StatusCode::OK);
    let html = index.text();
    assert!(html.contains(r#"href="/updates/ferrum-public-alpha""#));
    assert!(html.contains(r#"type="application/atom+xml""#));
    assert!(html.contains(r#"id="follow""#));
    assert!(html.contains(r#"href="/updates/feed.xml""#));

    let post = get("/updates/ferrum-public-alpha").await;
    assert_eq!(post.status, StatusCode::OK);
    let html = post.text();
    assert!(html.contains("<h1 id=\"post-title\">Ferrum is in public alpha</h1>"));
    assert!(html.contains(r#"<time datetime="2026-11-11">11 November 2026</time>"#));
    assert!(html.contains(r#"<meta property="og:type" content="article">"#));
    assert!(html.contains(r#"href="/updates" aria-current="page""#));
    let article_end = html.find("</article>").unwrap();
    assert!(html.find(r#"id="follow""#).unwrap() > article_end);

    assert_eq!(
        get("/updates/no-such-post").await.status,
        StatusCode::NOT_FOUND
    );
}

#[tokio::test]
async fn feed_is_valid_atom() {
    let config = Config::from_vars(|k| (k == "SITE_URL").then(|| "https://ferrum.test".to_owned()));
    let req = Request::get("/updates/feed.xml")
        .body(Body::empty())
        .unwrap();
    let res = app(config).oneshot(req).await.unwrap();
    assert_eq!(res.status(), StatusCode::OK);
    assert!(
        res.headers()[header::CONTENT_TYPE]
            .to_str()
            .unwrap()
            .starts_with("application/atom+xml")
    );
    let body = res.into_body().collect().await.unwrap().to_bytes();
    let body = String::from_utf8_lossy(&body);
    let doc = roxmltree::Document::parse(&body).unwrap();
    let feed = doc.root_element();
    assert_eq!(feed.tag_name().name(), "feed");
    assert_eq!(
        feed.tag_name().namespace(),
        Some("http://www.w3.org/2005/Atom")
    );
    let entries: Vec<_> = feed
        .children()
        .filter(|n| n.has_tag_name("entry"))
        .collect();
    assert_eq!(entries.len(), web_ferrum::updates::POSTS.len());
    let first = entries[0];
    let text = |name: &str| {
        first
            .children()
            .find(|n| n.has_tag_name(name))
            .and_then(|n| n.text())
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(text("title"), "Ferrum is in public alpha");
    assert_eq!(
        text("id"),
        "https://ferrum.test/updates/ferrum-public-alpha"
    );
    assert_eq!(text("published"), "2026-11-11T00:00:00Z");
    assert!(text("content").contains("href=\"https://ferrum.test/ferrumapi\""));
}

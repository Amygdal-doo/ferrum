//! End-to-end HTTP tests against the full router. Releases come from fake
//! fetchers, so no test touches the network.
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
    releases::Releases,
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
    send_with(Releases::offline(), req).await
}

async fn send_with(releases: Releases, req: Request<Body>) -> Res {
    let res = app_with(Config::default(), releases)
        .oneshot(req)
        .await
        .unwrap();
    let status = res.status();
    let headers = res.headers().clone();
    let body = res.into_body().collect().await.unwrap().to_bytes().to_vec();
    Res {
        status,
        headers,
        body,
    }
}

const PAGES: [&str; 36] = [
    "/",
    "/apps",
    "/features",
    "/ferrumapi",
    "/ferrumgit",
    "/ferrumdata",
    "/ferrumterm",
    "/ferrumkit",
    "/ferrumcontainers",
    "/ferrumports",
    "/ferrumenv",
    "/ferrumagentic",
    "/ferrum-editor",
    "/ferrum-launcher",
    "/download",
    "/docs",
    "/docs/install",
    "/docs/shell-commands",
    "/docs/project-bar",
    "/docs/shortcuts",
    "/changelog",
    "/faq",
    "/security",
    "/compare",
    "/compare/ferrumcontainers-vs-docker-desktop",
    "/compare/ferrumapi-vs-postman",
    "/updates",
    "/updates/ferrum-public-alpha",
    "/updates/ferrum-editor-0-5-and-ferrumagentic",
    "/updates/onboarding-and-settings",
    "/updates/glass-gradients-and-pane-cards",
    "/updates/ferrumterm-0-7-menu-and-worktrees",
    "/updates/calmer-windows-agent-logos-and-fec",
    "/about",
    "/licenses",
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

fn fake_releases(result: Result<&'static str, &'static str>) -> Releases {
    Releases::new(Arc::new(move || {
        Box::pin(async move { result.map(str::to_owned).map_err(str::to_owned) })
    }))
}

async fn get_with(releases: Releases, uri: &str) -> Res {
    send_with(releases, Request::get(uri).body(Body::empty()).unwrap()).await
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
        ("/ferrumagentic", "FerrumAgentic", "agentic"),
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
    for path in ["/does-not-exist", "/compare/nope", "/docs/nope"] {
        let res = get(path).await;
        assert_eq!(res.status, StatusCode::NOT_FOUND, "{path}");
        assert!(res.header(header::CONTENT_TYPE).starts_with("text/html"));
        assert_eq!(res.header(header::CACHE_CONTROL), "no-cache");
        let html = res.text();
        assert!(html.contains(r#"<meta name="robots" content="noindex">"#));
        assert!(!html.contains(r#"rel="canonical""#));
        assert_eq!(html.matches("<h1").count(), 1);
    }
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
    assert!(res.text().contains("Disallow: /download/macos"));
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
    let doc = roxmltree::Document::parse(&xml).unwrap();
    let urls: Vec<_> = doc
        .root_element()
        .children()
        .filter(|n| n.has_tag_name("url"))
        .collect();
    assert_eq!(urls.len(), PAGES.len());
    for url in urls {
        let lastmod = url
            .children()
            .find(|n| n.has_tag_name("lastmod"))
            .and_then(|n| n.text())
            .unwrap_or_default();
        assert_eq!(lastmod.len(), 10, "lastmod {lastmod:?}");
        assert_eq!(lastmod.matches('-').count(), 2);
    }
    assert!(!xml.contains("/download/macos"));
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
async fn ferrumdock_was_renamed_ferrumcontainers() {
    let res = get("/ferrumdock").await;
    assert_eq!(res.status, StatusCode::PERMANENT_REDIRECT);
    assert_eq!(res.header(header::LOCATION), "/ferrumcontainers");
    let html = get("/ferrumcontainers").await.text();
    assert!(html.contains(">FerrumContainers</h1>"));
    assert!(
        html.contains("/assets/img/brand/dock.svg?v="),
        "logo file keeps its name"
    );
    for path in [
        "/",
        "/features",
        "/docs/shortcuts",
        "/docs/shell-commands",
        "/changelog",
    ] {
        assert!(!get(path).await.text().contains("FerrumDock"), "{path}");
    }
    assert!(!get("/sitemap.xml").await.text().contains("/ferrumdock"));
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
    let newest = web_ferrum::updates::newest_first()[0];
    let title_of = |entry: &roxmltree::Node| {
        entry
            .children()
            .find(|n| n.has_tag_name("title"))
            .and_then(|n| n.text())
            .unwrap_or_default()
            .to_owned()
    };
    assert_eq!(title_of(&entries[0]), newest.title);
    let first = *entries
        .iter()
        .find(|e| title_of(e) == "Ferrum is in public alpha")
        .unwrap();
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

/// Every `<script type="application/ld+json">` body on a page, parsed.
fn json_ld(html: &str) -> Vec<serde_json::Value> {
    const OPEN: &str = r#"<script type="application/ld+json">"#;
    html.split(OPEN)
        .skip(1)
        .map(|rest| {
            let body = &rest[..rest.find("</script>").unwrap()];
            serde_json::from_str(body).unwrap_or_else(|e| panic!("bad JSON-LD {e}: {body}"))
        })
        .collect()
}

fn ld_types(values: &[serde_json::Value]) -> Vec<String> {
    values
        .iter()
        .map(|v| v["@type"].as_str().unwrap_or_default().to_owned())
        .collect()
}

fn meta_content(html: &str, attr: &str, name: &str) -> Option<String> {
    let needle = format!(r#"<meta {attr}="{name}" content=""#);
    let start = html.find(&needle)? + needle.len();
    Some(html[start..start + html[start..].find('"')?].to_owned())
}

#[tokio::test]
async fn every_page_has_unique_seo_metadata() {
    let site = Config::from_vars(|k| (k == "SITE_URL").then(|| "https://ferrum.test".to_owned()));
    let mut titles = std::collections::HashSet::new();
    let mut descriptions = std::collections::HashSet::new();
    for path in PAGES {
        let req = Request::get(path).body(Body::empty()).unwrap();
        let res = app_with(site.clone(), Releases::offline())
            .oneshot(req)
            .await
            .unwrap();
        let body = res.into_body().collect().await.unwrap().to_bytes();
        let html = String::from_utf8_lossy(&body).into_owned();

        assert!(
            html.starts_with("<!DOCTYPE html><html lang=\"en\""),
            "{path}"
        );
        assert_eq!(html.matches("<h1").count(), 1, "{path}: one h1");
        let title = html
            .split("<title>")
            .nth(1)
            .and_then(|t| t.split("</title>").next());
        let title = title.unwrap_or_default().to_owned();
        assert!(
            title.len() >= 10 && title.len() <= 75,
            "{path}: title {title:?}"
        );
        assert!(titles.insert(title), "{path}: duplicate title");
        let description = meta_content(&html, "name", "description").unwrap_or_default();
        assert!(
            (50..=200).contains(&description.chars().count()),
            "{path}: description length {}",
            description.chars().count()
        );
        assert!(
            descriptions.insert(description),
            "{path}: duplicate description"
        );

        let canonical = format!(r#"<link rel="canonical" href="https://ferrum.test{path}">"#);
        assert!(html.contains(&canonical), "{path}: canonical");
        assert_eq!(
            meta_content(&html, "property", "og:url").as_deref(),
            Some(format!("https://ferrum.test{path}").as_str()),
            "{path}"
        );
        let og_image = meta_content(&html, "property", "og:image").unwrap_or_default();
        assert!(
            og_image.starts_with("https://ferrum.test/assets/img/"),
            "{path}"
        );
        assert_eq!(meta_content(&html, "name", "twitter:image"), Some(og_image));
        assert!(
            meta_content(&html, "name", "twitter:card").is_some(),
            "{path}"
        );
        assert!(html.contains(r#"<meta name="theme-color""#), "{path}");
        assert!(html.contains(r#"rel="icon""#), "{path}");

        let ld = json_ld(&html);
        let types = ld_types(&ld);
        if path == "/" {
            assert!(types.contains(&"Organization".to_owned()));
            assert!(types.contains(&"WebSite".to_owned()));
        } else {
            assert!(
                types.contains(&"BreadcrumbList".to_owned()),
                "{path}: {types:?}"
            );
        }
        for v in &ld {
            assert_eq!(v["@context"], "https://schema.org", "{path}");
        }
    }
}

#[tokio::test]
async fn app_pages_describe_a_free_macos_application() {
    for p in web_ferrum::products::ALL {
        let html = get(&p.path()).await.text();
        let ld = json_ld(&html);
        let app = ld
            .iter()
            .find(|v| v["@type"] == "SoftwareApplication")
            .unwrap_or_else(|| panic!("{}: no SoftwareApplication", p.slug));
        assert_eq!(app["name"], p.name);
        assert_eq!(app["operatingSystem"], "macOS");
        assert_eq!(app["applicationCategory"], "DeveloperApplication");
        assert_eq!(app["offers"]["price"], "0");
        assert_eq!(app["url"], format!("http://localhost:8080{}", p.path()));
        let crumbs = ld.iter().find(|v| v["@type"] == "BreadcrumbList").unwrap();
        assert_eq!(crumbs["itemListElement"][1]["name"], p.name);
        // Per-app social image, and it is served.
        let image = format!("/assets/img/og/{}.png", p.slug);
        assert!(
            html.contains(&format!("http://localhost:8080{image}?v=")),
            "{}",
            p.slug
        );
        let res = get(&image).await;
        assert_eq!(res.status, StatusCode::OK, "{image}");
        assert_eq!(res.header(header::CONTENT_TYPE), "image/png");
    }
}

#[tokio::test]
async fn faq_has_valid_faq_page_markup() {
    let html = get("/faq").await.text();
    let ld = json_ld(&html);
    let faq = ld.iter().find(|v| v["@type"] == "FAQPage").unwrap();
    let questions = faq["mainEntity"].as_array().unwrap();
    assert!(questions.len() >= 8);
    for q in questions {
        assert_eq!(q["@type"], "Question");
        let name = q["name"].as_str().unwrap();
        assert!(html.contains(&format!(">{name}</dt>")), "{name}");
        assert!(!q["acceptedAnswer"]["text"].as_str().unwrap().is_empty());
    }
}

#[tokio::test]
async fn posts_are_blog_postings() {
    let html = get("/updates/ferrum-public-alpha").await.text();
    let ld = json_ld(&html);
    let post = ld.iter().find(|v| v["@type"] == "BlogPosting").unwrap();
    assert_eq!(post["datePublished"], "2026-11-11");
    assert_eq!(post["headline"], "Ferrum is in public alpha");
}

#[tokio::test]
async fn docs_cover_install_commands_bar_and_shortcuts() {
    let install = get("/docs/install").await.text();
    assert!(install.contains("install.sh | bash"));
    assert!(install.contains(r#"href="/docs" aria-current="page""#));
    let shell = get("/docs/shell-commands").await.text();
    for cmd in [
        "fee", "fea", "feg", "fed", "fet", "fek", "fec", "fep", "fev", "fel",
    ] {
        assert!(shell.contains(&format!("<code>{cmd}</code>")), "{cmd}");
    }
    let bar = get("/docs/project-bar").await.text();
    assert!(bar.contains("~/.ferrum/projects.json"));
    assert!(bar.contains("Option Command O"));
    let keys = get("/docs/shortcuts").await.text();
    assert!(keys.contains("Shift Command P"));
    assert!(keys.contains(r##"href="#ferrumcontainers""##));
}

#[tokio::test]
async fn changelog_lists_releases_from_github() {
    let json = r#"[
      {"tag_name": "launcher-v0.6.0", "published_at": "2026-11-10T09:00:00Z",
       "body": "- Renamed from Ferrum Toolbox", "html_url": "https://github.com/x/launcher-v0.6.0",
       "assets": [{"name": "FerrumLauncher-0.6.0-aarch64.dmg",
                   "browser_download_url": "https://dl.test/0.6.0.dmg"}]},
      {"tag_name": "api-v0.2.0-alpha.1", "prerelease": true,
       "published_at": "2026-10-02T09:00:00Z", "body": "First alpha"}
    ]"#;
    let res = get_with(fake_releases(Ok(json)), "/changelog").await;
    assert_eq!(res.status, StatusCode::OK);
    let html = res.text();
    assert!(html.contains("Ferrum Launcher</a> 0.6.0"));
    assert!(html.contains("FerrumAPI</a> 0.2.0-alpha.1"));
    assert!(html.contains("<li>Renamed from Ferrum Toolbox</li>"));
    assert!(html.contains(">November 2026</h2>"));
    assert!(html.contains("https://github.com/x/launcher-v0.6.0"));
    assert!(!html.contains("can't be loaded"));
}

#[tokio::test]
async fn changelog_degrades_gracefully_offline() {
    let res = get("/changelog").await;
    assert_eq!(res.status, StatusCode::OK);
    let html = res.text();
    assert!(html.contains("can't be loaded"));
    assert!(html.contains("https://github.com/Amygdal-doo/ferrum-releases/releases"));
}

#[tokio::test]
async fn favicon_ico_points_to_the_png() {
    let res = get("/favicon.ico").await;
    assert_eq!(res.status, StatusCode::PERMANENT_REDIRECT);
    assert!(
        res.header(header::LOCATION)
            .starts_with("/assets/img/favicon-32.png?v=")
    );
    let manifest = get("/assets/site.webmanifest").await;
    assert_eq!(manifest.status, StatusCode::OK);
}

#[tokio::test]
async fn new_pages_are_linked_from_every_page() {
    let html = get("/").await.text();
    for path in [
        "/docs",
        "/changelog",
        "/faq",
        "/security",
        "/compare",
        "/docs/shortcuts",
    ] {
        assert!(html.contains(&format!(r#"href="{path}""#)), "{path}");
    }
}

#[tokio::test]
async fn licenses_page_lists_bundled_works_and_is_linked() {
    let res = get("/licenses").await;
    assert_eq!(res.status, StatusCode::OK);
    let html = res.text();
    assert_eq!(html.matches("<h1").count(), 1);
    for needle in [
        "Lucide",
        "IBM Plex Sans",
        "Lilex",
        "OFL",
        "ISC",
        "Apache-2.0",
        "MIT",
        "all rights reserved",
        "AMYGDAL d.o.o.",
        "<details",
        "axum",
        "maud",
    ] {
        assert!(html.contains(needle), "{needle}");
    }
    assert!(ld_types(&json_ld(&html)).contains(&"BreadcrumbList".to_owned()));

    let sitemap = get("/sitemap.xml").await.text();
    assert!(sitemap.contains("<loc>http://localhost:8080/licenses</loc>"));

    for path in ["/", "/privacy", "/licenses"] {
        let html = get(path).await.text();
        let footer = html.split("<footer").nth(1).unwrap_or_default();
        assert!(footer.contains(r#"href="/licenses""#), "{path}");
    }
    for path in ["/security", "/faq", "/about"] {
        let html = get(path).await.text();
        let main = html.split("<footer").next().unwrap_or_default();
        assert!(main.contains(r#"href="/licenses""#), "{path}");
    }
    let faq = get("/faq").await.text();
    assert!(faq.contains("proprietary"));
    assert!(!get("/about").await.text().contains("theme crates"));
}

#[test]
fn app_constructor_is_still_available() {
    // `app` reads from GitHub when used; building it doesn't fetch anything.
    let _ = app(Config::default());
}

#[tokio::test]
async fn apps_page_lists_every_app_and_nav_points_to_it() {
    let res = get("/apps").await;
    assert_eq!(res.status, StatusCode::OK);
    let html = res.text();
    assert!(html.contains("<title>All Ferrum apps | Ferrum</title>"));
    assert!(html.contains(r#"href="/apps" aria-current="page""#));
    for p in web_ferrum::products::ALL {
        assert!(
            html.contains(&format!(r#"href="{}""#, p.path())),
            "{}",
            p.slug
        );
        assert!(
            html.contains(&format!(r#"id="app-{}""#, p.slug)),
            "{}",
            p.slug
        );
    }
    let types = ld_types(&json_ld(&html));
    assert!(types.contains(&"ItemList".to_owned()), "{types:?}");
    assert!(types.contains(&"BreadcrumbList".to_owned()), "{types:?}");

    let home = get("/").await.text();
    assert!(home.contains(r#"href="/apps""#));
    assert!(!home.contains(r##"href="/#apps""##));
    assert_eq!(home.matches(r#"role="tab""#).count(), 4);

    let xml = get("/sitemap.xml").await.text();
    assert!(xml.contains("<loc>http://localhost:8080/apps</loc><lastmod>"));
}

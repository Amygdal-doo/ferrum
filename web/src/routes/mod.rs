//! Router assembly. Page handlers are thin: they call `pages::*::render`.

pub mod assets;
pub mod health;
pub mod middleware;
pub mod seo;

use std::sync::Arc;

use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    routing::get,
};
use maud::Markup;

use crate::{Config, pages, releases::MacosDownload, site};

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub macos_download: Arc<MacosDownload>,
}

impl AppState {
    pub fn site_url(&self) -> &str {
        &self.config.site_url
    }
}

/// Build the full application router (used by `main` and integration tests).
pub fn app(config: Config) -> Router {
    app_with(config, MacosDownload::github())
}

/// Like [`app`], with a custom toolbox download resolver (tests inject a fake fetcher).
pub fn app_with(config: Config, macos_download: MacosDownload) -> Router {
    let state = AppState {
        config: Arc::new(config),
        macos_download: Arc::new(macos_download),
    };
    let router = Router::new()
        .route("/", get(home))
        .route("/ferrumdata", get(ferrumdata))
        .route("/ferrumapi", get(ferrumapi))
        .route("/ferrum-ide", get(ferrum_ide))
        .route("/ferrumgit", get(ferrumgit))
        .route("/ferrumterm", get(ferrumterm))
        .route("/ferrum-toolbox", get(ferrum_toolbox))
        .route("/download", get(download))
        .route(site::MACOS_DOWNLOAD_PATH, get(download_macos))
        .route("/about", get(about))
        .route("/healthz", get(health::healthz))
        .route("/robots.txt", get(seo::robots))
        .route("/sitemap.xml", get(seo::sitemap))
        .route("/assets/{*path}", get(assets::serve))
        .fallback(not_found)
        .with_state(state);
    middleware::with_middleware(router)
}

async fn home(State(s): State<AppState>) -> Markup {
    pages::home::render(s.site_url())
}
async fn ferrumdata(State(s): State<AppState>) -> Markup {
    pages::ferrumdata::render(s.site_url())
}
async fn ferrumapi(State(s): State<AppState>) -> Markup {
    pages::ferrumapi::render(s.site_url())
}
async fn ferrum_ide(State(s): State<AppState>) -> Markup {
    pages::ferrum_ide::render(s.site_url())
}
async fn ferrumgit(State(s): State<AppState>) -> Markup {
    pages::ferrumgit::render(s.site_url())
}
async fn ferrumterm(State(s): State<AppState>) -> Markup {
    pages::ferrumterm::render(s.site_url())
}
async fn ferrum_toolbox(State(s): State<AppState>) -> Markup {
    pages::ferrum_toolbox::render(s.site_url())
}
async fn download(State(s): State<AppState>) -> Markup {
    pages::download::render(s.site_url())
}
/// 302 to the newest toolbox DMG (or the releases page). Not cached by clients,
/// so a new release is picked up as soon as the server-side cache expires.
async fn download_macos(
    State(s): State<AppState>,
) -> (StatusCode, [(header::HeaderName, String); 2]) {
    (
        StatusCode::FOUND,
        [
            (header::LOCATION, s.macos_download.url().await),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
    )
}
async fn about(State(s): State<AppState>) -> Markup {
    pages::about::render(s.site_url())
}
async fn not_found(
    State(s): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], Markup) {
    (
        StatusCode::NOT_FOUND,
        [(header::CACHE_CONTROL, "no-cache")],
        pages::not_found::render(s.site_url()),
    )
}

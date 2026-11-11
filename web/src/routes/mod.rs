//! Router assembly. Page handlers are thin: they call `pages::*::render`.

pub mod assets;
pub mod health;
pub mod middleware;
pub mod seo;
pub mod updates;

use std::sync::Arc;

use axum::{
    Router,
    extract::State,
    http::{StatusCode, header},
    response::Redirect,
    routing::get,
};
use maud::Markup;

use crate::{Config, pages, releases::MacosDownload, site};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub macos_download: Arc<MacosDownload>,
}

impl AppState {
    pub fn new(config: Config, macos_download: MacosDownload) -> Self {
        Self {
            config: Arc::new(config),
            macos_download: Arc::new(macos_download),
        }
    }

    pub fn site_url(&self) -> &str {
        &self.config.site_url
    }
}

/// Build the full application router (used by `main` and integration tests).
pub fn app(config: Config) -> Router {
    app_with(config, MacosDownload::github())
}

/// Like [`app`], with a custom launcher download resolver (tests inject a fake fetcher).
pub fn app_with(config: Config, macos_download: MacosDownload) -> Router {
    router(AppState::new(config, macos_download))
}

/// The router for an existing state.
pub fn router(state: AppState) -> Router {
    let router = Router::new()
        .route("/", get(home))
        .route("/ferrumdata", get(ferrumdata))
        .route("/ferrumapi", get(ferrumapi))
        .route("/ferrumgit", get(ferrumgit))
        .route("/ferrumterm", get(ferrumterm))
        .route("/ferrumkit", get(ferrumkit))
        .route("/ferrumdock", get(ferrumdock))
        .route("/ferrumports", get(ferrumports))
        .route("/ferrumenv", get(ferrumenv))
        .route("/ferrum-editor", get(ferrum_editor))
        .route("/ferrum-launcher", get(ferrum_launcher))
        .route(
            "/ferrum-toolbox",
            get(|| async { Redirect::permanent("/ferrum-launcher") }),
        )
        // Ferrum IDE was dropped; Ferrum Editor is the suite's code editor.
        .route(
            "/ferrum-ide",
            get(|| async { Redirect::permanent("/ferrum-editor") }),
        )
        // FerrumCI and FerrumLogs are now the CI and Logs pages inside Ferrum Editor.
        .route(
            "/ferrumci",
            get(|| async { Redirect::permanent(pages::ferrum_editor::CI_PATH) }),
        )
        .route(
            "/ferrumlogs",
            get(|| async { Redirect::permanent(pages::ferrum_editor::LOGS_PATH) }),
        )
        .route(pages::features::PATH, get(features))
        .route("/download", get(download))
        .route(site::MACOS_DOWNLOAD_PATH, get(download_macos))
        .route("/about", get(about))
        .route(crate::updates::INDEX_PATH, get(updates::index))
        .route(crate::updates::FEED_PATH, get(updates::feed))
        .route("/updates/{slug}", get(updates::post))
        .route(pages::privacy::PATH, get(privacy))
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
async fn privacy(State(s): State<AppState>) -> Markup {
    pages::privacy::render(s.site_url())
}
async fn ferrumdata(State(s): State<AppState>) -> Markup {
    pages::ferrumdata::render(s.site_url())
}
async fn ferrumapi(State(s): State<AppState>) -> Markup {
    pages::ferrumapi::render(s.site_url())
}
async fn ferrumgit(State(s): State<AppState>) -> Markup {
    pages::ferrumgit::render(s.site_url())
}
async fn ferrumterm(State(s): State<AppState>) -> Markup {
    pages::ferrumterm::render(s.site_url())
}
async fn ferrumkit(State(s): State<AppState>) -> Markup {
    pages::ferrumkit::render(s.site_url())
}
async fn ferrumdock(State(s): State<AppState>) -> Markup {
    pages::ferrumdock::render(s.site_url())
}
async fn ferrumports(State(s): State<AppState>) -> Markup {
    pages::ferrumports::render(s.site_url())
}
async fn ferrumenv(State(s): State<AppState>) -> Markup {
    pages::ferrumenv::render(s.site_url())
}
async fn ferrum_editor(State(s): State<AppState>) -> Markup {
    pages::ferrum_editor::render(s.site_url())
}
async fn ferrum_launcher(State(s): State<AppState>) -> Markup {
    pages::ferrum_launcher::render(s.site_url())
}
async fn features(State(s): State<AppState>) -> Markup {
    pages::features::render(s.site_url())
}
async fn download(State(s): State<AppState>) -> Markup {
    pages::download::render(s.site_url())
}
/// 302 to the newest launcher DMG (or the releases page). Not cached by clients,
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
pub(crate) async fn not_found(
    State(s): State<AppState>,
) -> (StatusCode, [(header::HeaderName, &'static str); 1], Markup) {
    (
        StatusCode::NOT_FOUND,
        [(header::CACHE_CONTROL, "no-cache")],
        pages::not_found::render(s.site_url()),
    )
}

//! Router assembly. Page handlers are thin: they call `pages::*::render`.

pub mod assets;
pub mod health;
pub mod middleware;
pub mod seo;
pub mod updates;

use std::sync::Arc;

use axum::{
    Router,
    extract::{Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::get,
};
use maud::Markup;

use crate::{Config, pages, products, releases::Releases, site};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    /// Releases of every app (launcher download, changelog, sitemap).
    pub releases: Arc<Releases>,
}

impl AppState {
    pub fn new(config: Config, releases: Releases) -> Self {
        Self {
            config: Arc::new(config),
            releases: Arc::new(releases),
        }
    }

    pub fn site_url(&self) -> &str {
        &self.config.site_url
    }
}

/// Build the full application router, reading releases from GitHub.
pub fn app(config: Config) -> Router {
    app_with(config, Releases::github())
}

/// Like [`app`], with another releases source (tests inject a fake fetcher,
/// so they never touch the network).
pub fn app_with(config: Config, releases: Releases) -> Router {
    router(AppState::new(config, releases))
}

/// The router for an existing state.
pub fn router(state: AppState) -> Router {
    let router = Router::new()
        .route("/", get(home))
        .route(pages::apps::PATH, get(apps))
        .route("/ferrumdata", get(ferrumdata))
        .route("/ferrumapi", get(ferrumapi))
        .route("/ferrumgit", get(ferrumgit))
        .route("/ferrumterm", get(ferrumterm))
        .route("/ferrumkit", get(ferrumkit))
        .route("/ferrumcontainers", get(ferrumcontainers))
        // FerrumDock was renamed FerrumContainers.
        .route(
            "/ferrumdock",
            get(|| async { Redirect::permanent(&products::FERRUMCONTAINERS.path()) }),
        )
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
        .route(pages::docs::PATH, get(docs))
        .route(pages::docs::install::PATH, get(docs_install))
        .route(pages::docs::shell_commands::PATH, get(docs_shell_commands))
        .route(pages::docs::project_bar::PATH, get(docs_project_bar))
        .route(pages::docs::shortcuts::PATH, get(docs_shortcuts))
        .route(pages::changelog::PATH, get(changelog))
        .route(pages::faq::PATH, get(faq))
        .route(pages::security::PATH, get(security))
        .route(pages::compare::PATH, get(compare_index))
        .route("/compare/{slug}", get(compare))
        .route(crate::updates::INDEX_PATH, get(updates::index))
        .route(crate::updates::FEED_PATH, get(updates::feed))
        .route("/updates/{slug}", get(updates::post))
        .route(pages::privacy::PATH, get(privacy))
        .route(pages::licenses::PATH, get(licenses))
        .route("/healthz", get(health::healthz))
        .route("/robots.txt", get(seo::robots))
        .route(
            "/favicon.ico",
            get(|| async { Redirect::permanent(&assets::url("img/favicon-32.png")) }),
        )
        .route("/sitemap.xml", get(seo::sitemap))
        .route("/assets/{*path}", get(assets::serve))
        .fallback(not_found)
        .with_state(state.clone());
    let site_url: Arc<str> = state.site_url().into();
    middleware::with_middleware(router.layer(axum::middleware::from_fn_with_state(
        site_url,
        middleware::canonical_host,
    )))
}

async fn home(State(s): State<AppState>) -> Markup {
    pages::home::render(s.site_url())
}
async fn apps(State(s): State<AppState>) -> Markup {
    pages::apps::render(s.site_url())
}
async fn privacy(State(s): State<AppState>) -> Markup {
    pages::privacy::render(s.site_url())
}
async fn licenses(State(s): State<AppState>) -> Markup {
    pages::licenses::render(s.site_url())
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
async fn ferrumcontainers(State(s): State<AppState>) -> Markup {
    pages::ferrumcontainers::render(s.site_url())
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
            (header::LOCATION, s.releases.launcher_dmg_url().await),
            (header::CACHE_CONTROL, "no-store".to_owned()),
        ],
    )
}
async fn about(State(s): State<AppState>) -> Markup {
    pages::about::render(s.site_url())
}
async fn docs(State(s): State<AppState>) -> Markup {
    pages::docs::render(s.site_url())
}
async fn docs_install(State(s): State<AppState>) -> Markup {
    pages::docs::install::render(s.site_url())
}
async fn docs_shell_commands(State(s): State<AppState>) -> Markup {
    pages::docs::shell_commands::render(s.site_url())
}
async fn docs_project_bar(State(s): State<AppState>) -> Markup {
    pages::docs::project_bar::render(s.site_url())
}
async fn docs_shortcuts(State(s): State<AppState>) -> Markup {
    pages::docs::shortcuts::render(s.site_url())
}
async fn changelog(State(s): State<AppState>) -> Markup {
    let entries = s.releases.changelog().await;
    pages::changelog::render(s.site_url(), entries.as_deref())
}
async fn faq(State(s): State<AppState>) -> Markup {
    pages::faq::render(s.site_url())
}
async fn security(State(s): State<AppState>) -> Markup {
    pages::security::render(s.site_url())
}
async fn compare_index(State(s): State<AppState>) -> Markup {
    pages::compare::index(s.site_url())
}
async fn compare(State(s): State<AppState>, Path(slug): Path<String>) -> Response {
    match pages::compare::find(&slug) {
        Some(c) => pages::compare::render(s.site_url(), c).into_response(),
        None => not_found(State(s)).await.into_response(),
    }
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

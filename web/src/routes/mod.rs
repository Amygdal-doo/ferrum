//! Router assembly. Page handlers are thin: they call `pages::*::render`.

pub mod accounts_proxy;
pub mod assets;
pub mod health;
pub mod middleware;
pub mod ping;
pub mod seo;
pub mod updates;

use std::sync::Arc;

use axum::{
    Router,
    extract::{DefaultBodyLimit, Path, State},
    http::{StatusCode, header},
    response::{IntoResponse, Redirect, Response},
    routing::{any, get, post},
};
use maud::Markup;

use crate::{Config, pages, products, releases::Releases, site, usage::Usage};

#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    /// Releases of every app (launcher download, changelog, sitemap).
    pub releases: Arc<Releases>,
    /// Anonymous usage counts (`/api/ping`, `/stats`); stores nothing
    /// without `DATABASE_URL`.
    pub usage: Arc<Usage>,
    /// Client for the accounts service behind `ACCOUNTS_URL`.
    pub accounts_client: reqwest::Client,
}

impl AppState {
    pub fn new(config: Config, releases: Releases) -> Self {
        let usage = Usage::from_url(config.database_url.as_deref());
        if let Some(id) = &config.ga_measurement_id {
            crate::analytics::enable(id);
        }
        Self {
            config: Arc::new(config),
            releases: Arc::new(releases),
            usage: Arc::new(usage),
            accounts_client: accounts_proxy::client(),
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
        .route("/ferrumvariables", get(ferrumvariables))
        // FerrumEnv was renamed FerrumVariables.
        .route(
            "/ferrumenv",
            get(|| async { Redirect::permanent(&products::FERRUMVARIABLES.path()) }),
        )
        .route("/ferrumengineer", get(ferrumengineer))
        // FerrumAgentic was renamed FerrumVibe, then FerrumEngineer.
        .route(
            "/ferrumvibe",
            get(|| async { Redirect::permanent(&products::FERRUMENGINEER.path()) }),
        )
        .route(
            "/ferrumagentic",
            get(|| async { Redirect::permanent(&products::FERRUMENGINEER.path()) }),
        )
        .route("/ferrum-system", get(ferrum_system))
        .route("/ferrumide", get(ferrumide))
        // Ferrum Editor was renamed FerrumIDE. (Ferrum IDE, an earlier app
        // that was dropped, had /ferrum-ide.)
        .route(
            "/ferrum-editor",
            get(|| async { Redirect::permanent(&products::FERRUMIDE.path()) }),
        )
        .route(
            "/ferrum-ide",
            get(|| async { Redirect::permanent(&products::FERRUMIDE.path()) }),
        )
        .route("/ferrum-launcher", get(ferrum_launcher))
        .route(
            "/ferrum-toolbox",
            get(|| async { Redirect::permanent("/ferrum-launcher") }),
        )
        // FerrumCI and FerrumLogs are now the CI and Logs pages inside FerrumIDE.
        .route(
            "/ferrumci",
            get(|| async { Redirect::permanent(pages::ferrumide::CI_PATH) }),
        )
        .route(
            "/ferrumlogs",
            get(|| async { Redirect::permanent(pages::ferrumide::LOGS_PATH) }),
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
        .route(pages::stats::PATH, get(stats))
        .route(
            ping::PATH,
            post(ping::ping).layer(DefaultBodyLimit::max(crate::usage::MAX_BODY_BYTES)),
        )
        // Accounts live in a separate service; see `accounts_proxy`.
        .route("/login", any(accounts_proxy::proxy))
        .route("/signup", any(accounts_proxy::proxy))
        .route("/logout", any(accounts_proxy::proxy))
        .route("/account", any(accounts_proxy::proxy))
        .route("/account/{*rest}", any(accounts_proxy::proxy))
        .route("/auth/{*rest}", any(accounts_proxy::proxy))
        .route("/oauth/{*rest}", any(accounts_proxy::proxy))
        .route("/api/v1/{*rest}", any(accounts_proxy::proxy))
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
async fn stats(State(s): State<AppState>) -> Markup {
    let (usage, downloads) = tokio::join!(s.usage.aggregates(), s.releases.download_totals());
    let view = match usage {
        Some(counts) => pages::stats::Usage::Counts(counts),
        None if s.usage.enabled() => pages::stats::Usage::Unavailable,
        None => pages::stats::Usage::NotCollected,
    };
    pages::stats::render(s.site_url(), &view, downloads.as_ref())
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
async fn ferrum_system(State(s): State<AppState>) -> Markup {
    pages::ferrum_system::render(s.site_url())
}

async fn ferrumports(State(s): State<AppState>) -> Markup {
    pages::ferrumports::render(s.site_url())
}
async fn ferrumvariables(State(s): State<AppState>) -> Markup {
    pages::ferrumvariables::render(s.site_url())
}
async fn ferrumengineer(State(s): State<AppState>) -> Markup {
    pages::ferrumengineer::render(s.site_url())
}
async fn ferrumide(State(s): State<AppState>) -> Markup {
    pages::ferrumide::render(s.site_url())
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

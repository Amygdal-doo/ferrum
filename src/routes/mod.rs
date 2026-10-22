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

use crate::{Config, pages};

#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
}

impl AppState {
    pub fn site_url(&self) -> &str {
        &self.config.site_url
    }
}

/// Build the full application router (used by `main` and integration tests).
pub fn app(config: Config) -> Router {
    let state = AppState {
        config: Arc::new(config),
    };
    let router = Router::new()
        .route("/", get(home))
        .route("/ferrumdata", get(ferrumdata))
        .route("/ferrumapi", get(ferrumapi))
        .route("/ferrum-ide", get(ferrum_ide))
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

//! Base HTML document shared by every page.
//!
//! Pages build a [`PageMeta`] and their `<main>` content, then call [`page`].
//! Nav and footer come from `crate::components`.

use maud::{DOCTYPE, Markup, html};

use crate::{components, routes::assets, site, updates};

/// Per-page metadata for `<head>` (title, description, canonical + OG tags).
#[derive(Debug, Clone)]
pub struct PageMeta {
    /// Page title without the site suffix, e.g. "FerrumData". Home passes `None`.
    pub title: Option<String>,
    pub description: String,
    /// Absolute path of the page, e.g. "/ferrumdata". Used for canonical URL and active nav.
    pub path: String,
    /// Extra page-specific stylesheet/body class hook, e.g. "page-home".
    pub body_class: &'static str,
    /// Emit `<meta name="robots" content="noindex">` (used by the 404 page).
    pub noindex: bool,
    /// Publication date (`YYYY-MM-DD`) of an article; switches `og:type` to `article`.
    pub published: Option<&'static str>,
}

impl PageMeta {
    pub fn new(
        title: Option<&str>,
        description: &str,
        path: &str,
        body_class: &'static str,
    ) -> Self {
        Self {
            title: title.map(str::to_owned),
            description: description.to_owned(),
            path: path.to_owned(),
            body_class,
            noindex: false,
            published: None,
        }
    }

    /// Mark the page as an article published on `date` (`YYYY-MM-DD`).
    pub fn article(mut self, date: &'static str) -> Self {
        self.published = Some(date);
        self
    }

    /// Mark the page as not indexable by search engines.
    pub fn noindex(mut self) -> Self {
        self.noindex = true;
        self
    }

    pub fn full_title(&self) -> String {
        match &self.title {
            Some(t) => format!("{t} · {}", site::SITE_NAME),
            None => format!("{} · {}", site::SITE_NAME, site::SITE_TAGLINE),
        }
    }
}

/// Stylesheets in cascade order. Each file is owned by one area of the codebase.
const STYLESHEETS: [&str; 6] = [
    "css/tokens.css",
    "css/base.css",
    "css/components.css",
    "css/pages.css",
    "css/features.css",
    "css/updates.css",
];

/// Render a complete HTML document. `site_url` has no trailing slash.
pub fn page(site_url: &str, meta: &PageMeta, main: Markup) -> Markup {
    let title = meta.full_title();
    let canonical = format!("{site_url}{}", meta.path);
    let og_image = format!("{site_url}{}", assets::url("img/og.png"));
    html! {
        (DOCTYPE)
        html lang="en" data-theme="dark" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) }
                meta name="description" content=(meta.description);
                @if meta.noindex {
                    meta name="robots" content="noindex";
                } @else {
                    link rel="canonical" href=(canonical);
                }
                meta name="color-scheme" content="dark light";
                meta name="theme-color" content="#0d1520";
                @if let Some(date) = meta.published {
                    meta property="og:type" content="article";
                    meta property="article:published_time" content=(date);
                } @else {
                    meta property="og:type" content="website";
                }
                meta property="og:site_name" content=(site::SITE_NAME);
                meta property="og:title" content=(title);
                meta property="og:description" content=(meta.description);
                meta property="og:url" content=(canonical);
                meta property="og:image" content=(og_image);
                meta property="og:image:width" content="1200";
                meta property="og:image:height" content="630";
                meta property="og:image:alt" content="Ferrum wordmark: one launcher, every tool";
                meta name="twitter:card" content="summary_large_image";
                link rel="icon" href=(assets::url("img/favicon.svg")) type="image/svg+xml";
                link rel="apple-touch-icon" href=(assets::url("img/apple-touch-icon.png"));
                link rel="alternate" type="application/atom+xml" title=(updates::FEED_TITLE)
                    href=(format!("{site_url}{}", updates::FEED_PATH));
                link rel="preconnect" href="https://fonts.googleapis.com";
                link rel="preconnect" href="https://fonts.gstatic.com" crossorigin;
                link rel="stylesheet" href="https://fonts.googleapis.com/css2?family=Big+Shoulders+Display:wght@700;800&family=Hanken+Grotesk:wght@400;500;600&family=JetBrains+Mono:wght@400;600&display=swap";
                @for sheet in STYLESHEETS {
                    link rel="stylesheet" href=(assets::url(sheet));
                }
                // Runs before first paint to apply the saved theme (no flash).
                script src=(assets::url("js/theme-init.js")) {}
                script src=(assets::url("js/site.js")) defer {}
                script src=(assets::url("js/filings.js")) defer {}
            }
            body class=(meta.body_class) {
                a class="skip-link" href="#main" { "Skip to content" }
                (components::nav::render(&meta.path))
                main id="main" { (main) }
                (components::footer::render())
            }
        }
    }
}

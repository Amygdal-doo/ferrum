//! Base HTML document shared by every page.
//!
//! Pages build a [`PageMeta`] and their `<main>` content, then call [`page`].
//! The head carries the SEO set: unique title and description, canonical URL
//! (from `SITE_URL`), Open Graph and Twitter cards, theme colours, icons and
//! JSON-LD (`BreadcrumbList` from the trail plus anything the page adds).

use maud::{DOCTYPE, Markup, html};
use serde_json::Value;

use crate::components::{self, breadcrumb::Crumb};
use crate::{routes::assets, seo, site, updates};

/// Default social image (1200×630) and its description.
pub const DEFAULT_OG_IMAGE: &str = "img/og.png";
const DEFAULT_OG_ALT: &str = "Ferrum wordmark: one launcher, every tool";

/// Per-page metadata for `<head>`.
#[derive(Debug, Clone)]
pub struct PageMeta {
    /// Page title without the site suffix, e.g. "FerrumData". Home passes `None`.
    pub title: Option<String>,
    pub description: String,
    /// Absolute path of the page, e.g. "/ferrumdata". Used for canonical URL and active nav.
    pub path: String,
    /// Page-specific body class hook, e.g. "page-home".
    pub body_class: &'static str,
    /// Emit `<meta name="robots" content="noindex">` (used by the 404 page).
    pub noindex: bool,
    /// Publication date (`YYYY-MM-DD`) of an article; switches `og:type` to `article`.
    pub published: Option<&'static str>,
    /// Breadcrumb trail after Home; the last crumb is this page.
    pub trail: Vec<Crumb>,
    /// Social image: embedded asset path and its alt text.
    pub og_image: (String, String),
    /// Extra JSON-LD objects for this page.
    pub json_ld: Vec<Value>,
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
            trail: Vec::new(),
            og_image: (DEFAULT_OG_IMAGE.to_owned(), DEFAULT_OG_ALT.to_owned()),
            json_ld: Vec::new(),
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

    /// Breadcrumb trail after Home (emits `BreadcrumbList` JSON-LD).
    pub fn trail(mut self, trail: Vec<Crumb>) -> Self {
        self.trail = trail;
        self
    }

    /// Use another embedded image (1200×630) for Open Graph and Twitter.
    pub fn og_image(mut self, asset: &str, alt: &str) -> Self {
        self.og_image = (asset.to_owned(), alt.to_owned());
        self
    }

    pub fn json_ld(mut self, value: Value) -> Self {
        self.json_ld.push(value);
        self
    }

    pub fn full_title(&self) -> String {
        match &self.title {
            Some(t) => format!("{t} | {}", site::SITE_NAME),
            None => format!("{}: {}", site::SITE_NAME, site::SITE_TITLE),
        }
    }

    /// Absolute URL of the social image.
    pub fn og_image_url(&self, site_url: &str) -> String {
        format!("{site_url}{}", assets::url(&self.og_image.0))
    }
}

/// Stylesheets in cascade order. Each file is owned by one area of the codebase.
const STYLESHEETS: [&str; 7] = [
    "css/tokens.css",
    "css/base.css",
    "css/components.css",
    "css/pages.css",
    "css/docs.css",
    "css/features.css",
    "css/updates.css",
];

/// Theme colours for the browser UI, matching `--bg` in tokens.css.
const THEME_DARK: &str = "#0d1520";
const THEME_LIGHT: &str = "#edf1f5";

/// Render a complete HTML document. `site_url` has no trailing slash.
pub fn page(site_url: &str, meta: &PageMeta, main: Markup) -> Markup {
    let title = meta.full_title();
    let canonical = format!("{site_url}{}", meta.path);
    let og_image = meta.og_image_url(site_url);
    let og_alt = &meta.og_image.1;
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
                meta name="theme-color" content=(THEME_DARK) media="(prefers-color-scheme: dark)";
                meta name="theme-color" content=(THEME_LIGHT) media="(prefers-color-scheme: light)";
                @if let Some(date) = meta.published {
                    meta property="og:type" content="article";
                    meta property="article:published_time" content=(date);
                } @else {
                    meta property="og:type" content="website";
                }
                meta property="og:site_name" content=(site::SITE_NAME);
                meta property="og:locale" content="en_US";
                meta property="og:title" content=(title);
                meta property="og:description" content=(meta.description);
                meta property="og:url" content=(canonical);
                meta property="og:image" content=(og_image);
                meta property="og:image:width" content="1200";
                meta property="og:image:height" content="630";
                meta property="og:image:alt" content=(og_alt);
                meta name="twitter:card" content="summary_large_image";
                meta name="twitter:title" content=(title);
                meta name="twitter:description" content=(meta.description);
                meta name="twitter:image" content=(og_image);
                meta name="twitter:image:alt" content=(og_alt);
                link rel="icon" href=(assets::url("img/favicon.svg")) type="image/svg+xml";
                link rel="icon" href=(assets::url("img/favicon-32.png")) type="image/png" sizes="32x32";
                link rel="apple-touch-icon" href=(assets::url("img/apple-touch-icon.png"));
                link rel="manifest" href=(assets::url("site.webmanifest"));
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
                @if !meta.noindex && !meta.trail.is_empty() {
                    (seo::script(&seo::breadcrumb_list(site_url, &meta.trail)))
                }
                @for value in &meta.json_ld {
                    (seo::script(value))
                }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn head_has_social_cards_and_breadcrumbs() {
        let meta = PageMeta::new(Some("FAQ"), "Answers", "/faq", "page-faq")
            .trail(vec![Crumb::new("FAQ", "/faq")]);
        let html = page("https://f.test", &meta, html! {}).into_string();
        assert!(html.contains("<title>FAQ | Ferrum</title>"));
        assert!(html.contains(r#"<link rel="canonical" href="https://f.test/faq">"#));
        assert!(html.contains(r#"<meta name="twitter:card" content="summary_large_image">"#));
        assert!(
            html.contains(
                r#"<meta property="og:image" content="https://f.test/assets/img/og.png?v="#
            )
        );
        assert!(html.contains(r#""@type":"BreadcrumbList""#));
        assert!(html.contains(r#"<html lang="en""#));
    }

    #[test]
    fn noindex_pages_skip_canonical_and_breadcrumbs() {
        let meta = PageMeta::new(Some("x"), "y", "/x", "")
            .trail(vec![Crumb::new("x", "/x")])
            .noindex();
        let html = page("https://f.test", &meta, html! {}).into_string();
        assert!(!html.contains("rel=\"canonical\""));
        assert!(!html.contains("BreadcrumbList"));
    }
}

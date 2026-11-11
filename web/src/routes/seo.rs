//! robots.txt and sitemap.xml. Both answer HEAD too (axum's `get` does).

use axum::{extract::State, http::header, response::IntoResponse};

use super::AppState;
use crate::{products, updates};

/// Non-product pages listed in the sitemap: home first, privacy last.
/// Posts (`/updates/<slug>`) follow `/updates`.
const STATIC_PATHS: [&str; 6] = [
    "/",
    "/features",
    "/download",
    "/updates",
    "/about",
    "/privacy",
];

pub async fn robots(State(s): State<AppState>) -> impl IntoResponse {
    let body = format!(
        "User-agent: *\nAllow: /\nSitemap: {}/sitemap.xml\n",
        s.site_url()
    );
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body)
}

/// Every public page path: "/", each product, then the other static pages,
/// with every post right after "/updates".
pub fn sitemap_paths() -> Vec<String> {
    let mut paths = vec![STATIC_PATHS[0].to_owned()];
    paths.extend(products::ALL.iter().map(|p| p.path()));
    for p in &STATIC_PATHS[1..] {
        paths.push((*p).to_owned());
        if *p == updates::INDEX_PATH {
            paths.extend(updates::newest_first().iter().map(|post| post.path()));
        }
    }
    paths
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn sitemap_xml(site_url: &str) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for p in sitemap_paths() {
        let loc = xml_escape(&format!("{site_url}{p}"));
        xml.push_str(&format!("  <url><loc>{loc}</loc></url>\n"));
    }
    xml.push_str("</urlset>\n");
    xml
}

pub async fn sitemap(State(s): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        sitemap_xml(s.site_url()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitemap_covers_home_products_static_pages_and_posts() {
        let paths = sitemap_paths();
        assert!(paths.iter().any(|p| p == "/download"));
        assert!(paths.iter().any(|p| p == "/updates"));
        for post in updates::POSTS {
            assert!(paths.contains(&post.path()));
        }
        assert_eq!(paths.first().map(String::as_str), Some("/"));
        assert_eq!(paths.last().map(String::as_str), Some("/privacy"));
        assert_eq!(
            paths.len(),
            products::ALL.len() + STATIC_PATHS.len() + updates::POSTS.len()
        );
    }

    #[test]
    fn sitemap_escapes_locations() {
        let xml = sitemap_xml("https://a.test/?x=1&y=2");
        assert!(xml.contains("&amp;y=2"));
    }
}

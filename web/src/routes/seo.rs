//! robots.txt and sitemap.xml. Both answer HEAD too (axum's `get` does).

use axum::{extract::State, http::header, response::IntoResponse};

use super::AppState;
use crate::products;

/// Non-product pages listed in the sitemap.
const STATIC_PATHS: [&str; 2] = ["/", "/about"];

pub async fn robots(State(s): State<AppState>) -> impl IntoResponse {
    let body = format!(
        "User-agent: *\nAllow: /\nSitemap: {}/sitemap.xml\n",
        s.site_url()
    );
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body)
}

/// Every public page path: "/", each product, then "/about".
pub fn sitemap_paths() -> Vec<String> {
    let (home, rest) = STATIC_PATHS.split_at(1);
    home.iter()
        .map(|p| (*p).to_owned())
        .chain(products::ALL.iter().map(|p| p.path()))
        .chain(rest.iter().map(|p| (*p).to_owned()))
        .collect()
}

fn xml_escape(s: &str) -> String {
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
    fn sitemap_covers_home_products_and_about() {
        let paths = sitemap_paths();
        assert_eq!(paths.first().map(String::as_str), Some("/"));
        assert_eq!(paths.last().map(String::as_str), Some("/about"));
        assert_eq!(paths.len(), products::ALL.len() + STATIC_PATHS.len());
    }

    #[test]
    fn sitemap_escapes_locations() {
        let xml = sitemap_xml("https://a.test/?x=1&y=2");
        assert!(xml.contains("&amp;y=2"));
    }
}

//! robots.txt and sitemap.xml. Both answer HEAD too (axum's `get` does).

use axum::{extract::State, http::header, response::IntoResponse};

use super::AppState;
use crate::pages::{
    apps, changelog, compare, docs, faq, features, licenses, privacy, security, stats,
};
use crate::{products, site, updates};

/// One sitemap entry: path and `lastmod` (`YYYY-MM-DD`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub path: String,
    pub lastmod: String,
}

fn entry(path: &str, lastmod: &str) -> Entry {
    Entry {
        path: path.to_owned(),
        lastmod: lastmod.to_owned(),
    }
}

pub async fn robots(State(s): State<AppState>) -> impl IntoResponse {
    let body = format!(
        "User-agent: *\nAllow: /\n# Redirects to a GitHub download.\nDisallow: {}\nSitemap: {}/sitemap.xml\n",
        site::MACOS_DOWNLOAD_PATH,
        s.site_url()
    );
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body)
}

/// Every public page: home, the apps, the guides and info pages, the
/// updates with every post, then usage stats, about, licences and privacy. `changelog_date` is the
/// newest release already known, if any.
pub fn sitemap_entries(changelog_date: Option<&str>) -> Vec<Entry> {
    let content = site::CONTENT_UPDATED;
    let newest_post = updates::newest_first().first().map_or(content, |p| p.date);
    let mut entries = vec![entry("/", content), entry(apps::PATH, content)];
    entries.extend(products::ALL.iter().map(|p| entry(&p.path(), content)));
    entries.push(entry(features::PATH, content));
    entries.push(entry(site::DOWNLOAD_PATH, content));
    entries.push(entry(docs::PATH, content));
    entries.extend(docs::DOCS.iter().map(|d| entry(d.path, content)));
    entries.push(entry(changelog::PATH, changelog_date.unwrap_or(content)));
    entries.push(entry(faq::PATH, content));
    entries.push(entry(security::PATH, content));
    entries.push(entry(compare::PATH, content));
    entries.extend(
        compare::COMPARISONS
            .iter()
            .map(|c| entry(&c.path(), content)),
    );
    entries.push(entry(updates::INDEX_PATH, newest_post));
    entries.extend(
        updates::newest_first()
            .iter()
            .map(|p| entry(&p.path(), p.date)),
    );
    entries.push(entry(stats::PATH, content));
    entries.push(entry("/about", content));
    entries.push(entry(licenses::PATH, content));
    entries.push(entry(privacy::PATH, privacy::UPDATED));
    entries
}

/// Paths only, in sitemap order.
pub fn sitemap_paths() -> Vec<String> {
    sitemap_entries(None).into_iter().map(|e| e.path).collect()
}

pub fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

pub fn sitemap_xml(site_url: &str, changelog_date: Option<&str>) -> String {
    let mut xml = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    for e in sitemap_entries(changelog_date) {
        let loc = xml_escape(&format!("{site_url}{}", e.path));
        let lastmod = xml_escape(&e.lastmod);
        xml.push_str(&format!(
            "  <url><loc>{loc}</loc><lastmod>{lastmod}</lastmod></url>\n"
        ));
    }
    xml.push_str("</urlset>\n");
    xml
}

pub async fn sitemap(State(s): State<AppState>) -> impl IntoResponse {
    let changelog_date = s.releases.newest_cached_date();
    (
        [(header::CONTENT_TYPE, "application/xml; charset=utf-8")],
        sitemap_xml(s.site_url(), changelog_date.as_deref()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sitemap_covers_every_page_once() {
        let paths = sitemap_paths();
        for p in [
            "/download",
            "/apps",
            "/updates",
            "/docs",
            "/docs/install",
            "/changelog",
            "/faq",
        ] {
            assert!(paths.iter().any(|x| x == p), "{p}");
        }
        for post in updates::POSTS {
            assert!(paths.contains(&post.path()));
        }
        assert_eq!(paths.first().map(String::as_str), Some("/"));
        assert_eq!(paths.last().map(String::as_str), Some("/privacy"));
        let mut unique = paths.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(unique.len(), paths.len(), "no duplicates");
    }

    #[test]
    fn every_entry_has_a_date() {
        for e in sitemap_entries(Some("2026-12-01")) {
            assert_eq!(e.lastmod.len(), 10, "{}", e.path);
            if e.path == "/changelog" {
                assert_eq!(e.lastmod, "2026-12-01");
            }
        }
    }

    #[test]
    fn sitemap_escapes_locations() {
        let xml = sitemap_xml("https://a.test/?x=1&y=2", None);
        assert!(xml.contains("&amp;y=2"));
        assert!(xml.contains("<lastmod>"));
    }
}

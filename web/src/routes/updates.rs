//! `/updates`, `/updates/{slug}` and the Atom feed at `/updates/feed.xml`.

use axum::{
    extract::{Path, State},
    http::header,
    response::{IntoResponse, Response},
};

use super::AppState;
use super::seo::xml_escape;
use crate::pages;
use crate::updates::{self, FEED_PATH, FEED_TITLE, INDEX_PATH};

pub async fn index(State(s): State<AppState>) -> Response {
    pages::updates::index(s.site_url()).into_response()
}

pub async fn post(State(s): State<AppState>, Path(slug): Path<String>) -> Response {
    match updates::find(&slug) {
        Some(post) => pages::updates::post(s.site_url(), post).into_response(),
        None => super::not_found(State(s)).await.into_response(),
    }
}

/// `2026-11-11` → `2026-11-11T00:00:00Z`.
fn atom_time(date: &str) -> String {
    format!("{date}T00:00:00Z")
}

pub fn feed_xml(site_url: &str) -> String {
    let posts = updates::newest_first();
    let updated = posts
        .first()
        .map_or_else(|| atom_time("2026-11-11"), |p| atom_time(p.date));
    let mut xml = format!(
        "<?xml version=\"1.0\" encoding=\"utf-8\"?>\n\
         <feed xmlns=\"http://www.w3.org/2005/Atom\">\n\
         \x20 <title>{title}</title>\n\
         \x20 <subtitle>Announcements from the Ferrum team.</subtitle>\n\
         \x20 <link rel=\"self\" type=\"application/atom+xml\" href=\"{self_url}\"/>\n\
         \x20 <link rel=\"alternate\" type=\"text/html\" href=\"{index}\"/>\n\
         \x20 <id>{index}</id>\n\
         \x20 <updated>{updated}</updated>\n",
        title = xml_escape(FEED_TITLE),
        self_url = xml_escape(&format!("{site_url}{FEED_PATH}")),
        index = xml_escape(&format!("{site_url}{INDEX_PATH}")),
    );
    for p in posts {
        let url = xml_escape(&format!("{site_url}{}", p.path()));
        // Root-relative links in the body would break in feed readers.
        let content = (p.body)()
            .into_string()
            .replace("href=\"/", &format!("href=\"{site_url}/"))
            .replace("src=\"/", &format!("src=\"{site_url}/"));
        xml.push_str(&format!(
            "  <entry>\n\
             \x20   <title>{title}</title>\n\
             \x20   <link rel=\"alternate\" type=\"text/html\" href=\"{url}\"/>\n\
             \x20   <id>{url}</id>\n\
             \x20   <published>{time}</published>\n\
             \x20   <updated>{time}</updated>\n\
             \x20   <author><name>{author}</name></author>\n\
             \x20   <summary>{summary}</summary>\n\
             \x20   <content type=\"html\">{content}</content>\n\
             \x20 </entry>\n",
            title = xml_escape(p.title),
            time = atom_time(p.date),
            author = xml_escape(p.byline),
            summary = xml_escape(p.summary),
            content = xml_escape(&content),
        ));
    }
    xml.push_str("</feed>\n");
    xml
}

pub async fn feed(State(s): State<AppState>) -> impl IntoResponse {
    (
        [(header::CONTENT_TYPE, "application/atom+xml; charset=utf-8")],
        feed_xml(s.site_url()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_has_an_entry_per_post_with_absolute_links() {
        let xml = feed_xml("https://ferrum.test");
        assert_eq!(xml.matches("<entry>").count(), updates::POSTS.len());
        assert!(xml.contains("<id>https://ferrum.test/updates</id>"));
        assert!(!xml.contains("href=&quot;/"));
    }
}

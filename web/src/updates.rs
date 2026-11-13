//! Posts on `/updates`: announcements from the Ferrum team.
//!
//! Each post is Rust data plus a maud body in `pages::posts`. To publish one,
//! add a module there and an entry to [`POSTS`]; the index, the sitemap and
//! the Atom feed pick it up.

use maud::Markup;

use crate::pages::posts;

pub const INDEX_PATH: &str = "/updates";
pub const FEED_PATH: &str = "/updates/feed.xml";
pub const FEED_TITLE: &str = "Ferrum updates";

#[derive(Debug, Clone, Copy)]
pub struct Post {
    /// URL segment: `/updates/<slug>`.
    pub slug: &'static str,
    pub title: &'static str,
    /// Publication date, `YYYY-MM-DD`.
    pub date: &'static str,
    pub byline: &'static str,
    /// One or two sentences for the index, meta description and feed.
    pub summary: &'static str,
    pub body: fn() -> Markup,
}

impl Post {
    pub fn path(&self) -> String {
        format!("{INDEX_PATH}/{}", self.slug)
    }

    /// "11 November 2026".
    pub fn display_date(&self) -> String {
        display_date(self.date)
    }
}

/// Every post. Order doesn't matter: [`newest_first`] sorts them.
pub const POSTS: &[Post] = &[
    posts::public_alpha::POST,
    posts::editor_panes_agentic::POST,
    posts::onboarding_settings::POST,
];

/// Posts sorted by date, newest first (ties by title).
pub fn newest_first() -> Vec<&'static Post> {
    let mut all: Vec<&Post> = POSTS.iter().collect();
    all.sort_by(|a, b| b.date.cmp(a.date).then(a.title.cmp(b.title)));
    all
}

pub fn find(slug: &str) -> Option<&'static Post> {
    POSTS.iter().find(|p| p.slug == slug)
}

const MONTHS: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];

/// `2026-11-11` → `11 November 2026`; anything unparsable is returned as is.
pub fn display_date(iso: &str) -> String {
    let mut parts = iso.splitn(3, '-');
    let (Some(y), Some(m), Some(d)) = (parts.next(), parts.next(), parts.next()) else {
        return iso.to_owned();
    };
    match (m.parse::<usize>(), d.parse::<u32>()) {
        (Ok(m @ 1..=12), Ok(d)) => format!("{d} {} {y}", MONTHS[m - 1]),
        _ => iso.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_are_unique_and_url_safe() {
        for (i, a) in POSTS.iter().enumerate() {
            assert!(
                a.slug
                    .chars()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-'),
                "{}",
                a.slug
            );
            assert_eq!(a.date.len(), 10, "{}", a.slug);
            for b in &POSTS[i + 1..] {
                assert_ne!(a.slug, b.slug);
            }
        }
    }

    #[test]
    fn formats_dates() {
        assert_eq!(display_date("2026-11-11"), "11 November 2026");
        assert_eq!(display_date("2026-01-05"), "5 January 2026");
        assert_eq!(display_date("soon"), "soon");
    }

    #[test]
    fn newest_first_is_sorted() {
        let posts = newest_first();
        assert!(posts.windows(2).all(|w| w[0].date >= w[1].date));
        assert_eq!(posts.len(), POSTS.len());
    }
}

//! `/updates` (the list of posts) and `/updates/<slug>` (one post), each
//! ending with the "follow the updates" block that points to the Atom feed.

use maud::{Markup, html};

use crate::components::follow::{self, Size};
use crate::layout::{PageMeta, page};
use crate::updates::{self, FEED_PATH, INDEX_PATH, Post};

const INDEX_DESCRIPTION: &str = "Announcements from the Ferrum team: releases, what changed \
     and what is next for the Ferrum developer tools.";

pub fn index(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Updates"),
        INDEX_DESCRIPTION,
        INDEX_PATH,
        "page-updates",
    );
    page(
        site_url,
        &meta,
        html! {
            div class="container post-wrap" {
                header class="updates-head" {
                    h1 { "Updates" }
                    p class="lead" {
                        "Announcements from the Ferrum team. Follow along with the "
                        a href=(FEED_PATH) { "Atom feed" } "."
                    }
                }
                ol class="post-index" reversed {
                    @for post in updates::newest_first() {
                        li class="post-index__item" {
                            article aria-labelledby={ "post-" (post.slug) } {
                                p class="post-index__date" {
                                    time datetime=(post.date) { (post.display_date()) }
                                }
                                h2 class="post-index__title" id={ "post-" (post.slug) } {
                                    a href=(post.path()) { (post.title) }
                                }
                                p class="post-index__summary" { (post.summary) }
                            }
                        }
                    }
                }
                (follow::block(Size::Full, false))
            }
        },
    )
}

pub fn post(site_url: &str, post: &Post) -> Markup {
    let path = post.path();
    let meta = PageMeta::new(Some(post.title), post.summary, &path, "page-post").article(post.date);
    page(
        site_url,
        &meta,
        html! {
            div class="container post-wrap" {
                article class="post" aria-labelledby="post-title" {
                    header class="post__header" {
                        p class="post__crumb" { a href=(INDEX_PATH) { "Updates" } }
                        h1 id="post-title" { (post.title) }
                        p class="post__byline" {
                            span { (post.byline) }
                            " · "
                            time datetime=(post.date) { (post.display_date()) }
                        }
                    }
                    div class="post__body" { ((post.body)()) }
                }
                (follow::block(Size::Full, true))
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_lists_every_post_newest_first_and_the_feed() {
        let html = index("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        for p in updates::POSTS {
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
        }
        assert!(html.contains(r#"id="follow""#));
        assert!(html.contains(r#"href="/updates/feed.xml""#));
        assert!(!html.contains("<form"));
    }

    #[test]
    fn post_has_byline_article_meta_and_follow_block() {
        let p = &updates::POSTS[0];
        let html = post("https://example.test", p).into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(p.byline));
        assert!(html.contains(&format!(r#"<time datetime="{}">"#, p.date)));
        assert!(html.contains(r#"<meta property="og:type" content="article">"#));
        assert!(html.contains(&format!(
            r#"<link rel="canonical" href="https://example.test{}">"#,
            p.path()
        )));
        // The follow block comes after the article.
        let article_end = html.find("</article>").unwrap_or_default();
        assert!(html.find(r#"id="follow""#).unwrap_or_default() > article_end);
        assert!(!html.contains("<form"));
    }
}

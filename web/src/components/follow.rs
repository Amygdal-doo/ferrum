//! "Follow the updates" block, always at `#follow` (one per page).
//!
//! Ferrum has no mailing list: news is posted on `/updates` and in its Atom
//! feed, and this block points people there.
//!
//! Classes: `.follow` (`--compact`), `.follow__copy`, `.follow__title`,
//! `.follow__lead`, `.follow__actions`.

use maud::{Markup, html};

use crate::components::button::{self, Variant};
use crate::updates::{FEED_PATH, INDEX_PATH};

pub const ANCHOR: &str = "follow";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Size {
    /// End of a post and `/updates`.
    Full,
    /// Home page.
    Compact,
}

/// The follow block. `link_index` adds a button to `/updates` (off on
/// `/updates` itself).
pub fn block(size: Size, link_index: bool) -> Markup {
    let class = match size {
        Size::Full => "follow",
        Size::Compact => "follow follow--compact",
    };
    html! {
        section class=(class) id=(ANCHOR) aria-labelledby="follow-title" {
            div class="follow__copy" {
                h2 class="follow__title" id="follow-title" { "Follow Ferrum updates" }
                p class="follow__lead" {
                    "Releases and new builds are announced on the updates page. Add the Atom \
                     feed to your feed reader to get each post as it lands."
                }
            }
            div class="follow__actions" {
                (button::link("Atom feed", FEED_PATH, Variant::Primary))
                @if link_index {
                    (button::link("Read the updates", INDEX_PATH, Variant::Secondary))
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn points_to_the_feed_without_a_form() {
        let html = block(Size::Full, false).into_string();
        assert!(html.contains(r#"id="follow""#));
        assert!(html.contains(r#"href="/updates/feed.xml""#));
        assert!(!html.contains(r#"href="/updates""#));
        assert!(!html.contains("<form"));
        assert!(!html.to_lowercase().contains("email"));
    }

    #[test]
    fn compact_block_links_the_updates_page() {
        let html = block(Size::Compact, true).into_string();
        assert!(html.contains("follow--compact"));
        assert!(html.contains(r#"href="/updates""#));
    }
}

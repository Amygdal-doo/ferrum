//! `/privacy`: what this website stores and which services it involves.
//! Describes what the code does; no legal claims beyond that.

use maud::{Markup, html};

use crate::layout::{PageMeta, page};
use crate::site;

pub const PATH: &str = "/privacy";
const UPDATED: &str = "2026-11-11";

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Privacy"),
        "What the Ferrum website stores (nothing about you) and which services it involves.",
        PATH,
        "page-privacy",
    );
    page(
        site_url,
        &meta,
        html! {
            div class="container post-wrap" {
                article class="post" aria-labelledby="privacy-title" {
                    header class="post__header" {
                        h1 id="privacy-title" { "Privacy" }
                        p class="post__byline" {
                            "Last updated "
                            time datetime=(UPDATED) { (crate::updates::display_date(UPDATED)) }
                        }
                    }
                    div class="post__body" {
                        p class="post__intro" {
                            "This page covers this website. The Ferrum apps themselves keep their \
                             data on your computer."
                        }
                        h2 { "What we store" }
                        p {
                            "Nothing about you. There are no accounts, no forms and no mailing \
                             list. Like any web server, this one logs each request's method and \
                             path, without query strings."
                        }
                        h2 { "Services involved" }
                        ul class="post__list" {
                            li { strong { "Railway" } " hosts this website." }
                            li {
                                strong { "Google Fonts" } " serves the fonts on every page of \
                                 this site, so your browser contacts Google when it loads them."
                            }
                            li {
                                strong { "GitHub" } " hosts the downloads. Download links and the \
                                 launcher fetch releases from GitHub."
                            }
                        }
                        p { "This site sets no cookies and runs no analytics. The light or dark theme you pick is kept in your browser's local storage." }
                        h2 { "Contact" }
                        @match site::CONTACT_EMAIL {
                            Some(email) => p {
                                "Questions about your data: "
                                a href={ "mailto:" (email) } { (email) } "."
                            },
                            None => p {
                                "Ferrum is built by Amygdal. Reach the team through "
                                a href=(site::AMYGDAL_URL) rel="noopener" { "amygdal.com" } "."
                            },
                        }
                    }
                }
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn covers_the_site_without_a_mailing_list() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("Railway"));
        assert!(html.contains("no cookies"));
        for gone in ["Resend", "unsubscribe", "your email address"] {
            assert!(!html.contains(gone), "{gone}");
        }
    }
}

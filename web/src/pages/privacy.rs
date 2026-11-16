//! `/privacy`: what this website stores and which services it involves.
//! Describes what the code does; no legal claims beyond that.

use maud::{Markup, html};

use crate::components::{breadcrumb::Crumb, page_header::PageHeader};
use crate::layout::{PageMeta, page};
use crate::site;

pub const PATH: &str = "/privacy";
pub const UPDATED: &str = "2026-11-16";
/// Anchor of the usage-count section (the launcher links to it).
pub const USAGE_COUNTS_ID: &str = "usage-counts";
/// Anchor of the accounts section (the sign-in pages link to it).
pub const ACCOUNTS_ID: &str = "accounts";

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Privacy policy for this website"),
        "What the Ferrum website stores, what a Ferrum account keeps, the launcher's \
         anonymous daily install count and how to turn it off, and the services involved.",
        PATH,
        "page-privacy",
    )
    .trail(vec![Crumb::new("Privacy", PATH)]);
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Privacy")
                .lead(html! {
                    p {
                        "Last updated "
                        time datetime=(UPDATED) { (crate::updates::display_date(UPDATED)) }
                    }
                })
                .render())
            div class="container post-wrap" {
                article class="post" aria-labelledby="page-title" {
                    div class="post__body" {
                        p class="post__intro" {
                            "This page covers this website and the one count FerrumLauncher \
                             sends. The Ferrum apps keep their data on your computer and send no \
                             usage data of their own; see "
                            a href="/security" { "Security" } "."
                        }
                        h2 { "What we store" }
                        p {
                            "Nothing about you unless you sign in. There is no mailing list. \
                             Like any web server, this one logs each request's method and path, \
                             without query strings."
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
                                 launcher fetch releases from GitHub. It also confirms who you \
                                 are when you sign in, and serves your profile picture on your \
                                 account page."
                            }
                        }
                        p {
                            "This site runs no analytics and sets no cookies unless you sign in. \
                             The light or dark theme you pick is kept in your browser's local \
                             storage."
                        }
                        h2 id=(ACCOUNTS_ID) { "Ferrum accounts" }
                        p {
                            "An account is optional; every Ferrum app works without one. You \
                             sign in with GitHub. There are no passwords: GitHub confirms who \
                             you are, and Ferrum never sees your GitHub password or your \
                             repositories. An account keeps:"
                        }
                        ul class="post__list" {
                            li {
                                strong { "From GitHub" } ": your GitHub user ID and username, \
                                 your name, your primary verified e-mail address and the address \
                                 of your profile picture. They are refreshed each time you sign in."
                            }
                            li { strong { "Your Ferrum username" } ", which you can change." }
                            li {
                                strong { "Sessions" } ": one for each signed-in browser and each \
                                 Ferrum app on each computer, with the app, the computer's name \
                                 as the app reports it, and when it signed in and was last used. \
                                 Sign-in tokens are stored only as one-way hashes."
                            }
                        }
                        p {
                            "Signing in sets one cookie, " code { "ferrum_session" } ", that keeps \
                             this browser signed in, plus a short-lived one while GitHub \
                             confirms the sign-in. A session ends when you sign out, when you \
                             revoke it on your account's sessions page, or after 90 days \
                             without use. Accounts are never linked to the usage counts below. \
                             To delete your account and everything it keeps, ask us at the \
                             contact below."
                        }
                        h2 id=(USAGE_COUNTS_ID) { "Usage counts from FerrumLauncher" }
                        p {
                            "To know how many people use Ferrum, FerrumLauncher sends one small \
                             message to this website at most once a day (UTC). It is on by \
                             default; the launcher says so the first time it starts. The message \
                             holds exactly:"
                        }
                        ul class="post__list" {
                            li {
                                strong { "An install ID" } ": a random number the launcher makes \
                                 once and keeps in its state file. It isn't derived from your \
                                 computer or account."
                            }
                            li { strong { "The launcher version" } "." }
                            li { strong { "The OS and CPU type" } ": macOS, Linux or Windows; ARM64 or x86-64." }
                            li {
                                strong { "The installed apps" } ": each app's id and active \
                                 version (for example FerrumGIT 0.4.0)."
                            }
                        }
                        p {
                            "Nothing else: no names, file paths, project names, host names or \
                             e-mail addresses. The website stores one row per install per day \
                             and does not store your IP address or user agent with it. The \
                             totals are public on "
                            a href=(super::stats::PATH) { "Usage stats" } "."
                        }
                        p { "To turn it off, use either of these:" }
                        ul class="post__list" {
                            li {
                                "In the launcher, open Settings › Launcher and switch off "
                                strong { "Share anonymous usage counts" } " (or choose "
                                strong { "Turn off" } " on the first-run notice)."
                            }
                            li {
                                "Start the launcher with " code { "FERRUM_NO_ANALYTICS=1" } " (or "
                                code { "DO_NOT_TRACK=1" } ") in its environment."
                            }
                        }
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
        assert!(html.contains("sets no cookies unless you sign in"));
        for gone in ["Resend", "unsubscribe", "your email address"] {
            assert!(!html.contains(gone), "{gone}");
        }
    }

    #[test]
    fn explains_the_usage_count_and_how_to_turn_it_off() {
        let html = render("https://example.test").into_string();
        assert!(html.contains(r#"id="usage-counts""#));
        for text in [
            "install ID",
            "IP address",
            "FERRUM_NO_ANALYTICS=1",
            "DO_NOT_TRACK=1",
            "Share anonymous usage counts",
            r#"href="/stats""#,
        ] {
            assert!(html.contains(text), "{text}");
        }
    }

    #[test]
    fn explains_what_accounts_keep() {
        let html = render("https://example.test").into_string();
        assert!(html.contains(r#"id="accounts""#));
        for text in [
            "GitHub user ID",
            "primary verified e-mail",
            "profile picture",
            "Sessions",
            "no passwords",
            "ferrum_session",
            "90 days",
            "never linked to the usage counts",
            "delete your account",
        ] {
            assert!(html.contains(text), "{text}");
        }
    }
}

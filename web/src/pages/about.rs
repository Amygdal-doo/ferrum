//! About page: what Ferrum is, its principles, product status and credits.
//! No invented team members, metrics or customers.

use maud::{Markup, html};

use crate::components::{badge, card};
use crate::layout::{PageMeta, page};
use crate::{products, site};

const PRINCIPLES: &[(&str, &str)] = &[
    (
        "Local-first",
        "Your connections, requests and projects live on your machine. The tools are designed to work \
         without an account or a hosted backend.",
    ),
    (
        "Native performance",
        "Written in Rust and rendered on the GPU with GPUI. No Electron, no webview, no \
         bundled browser runtime pretending to be an app.",
    ),
    (
        "Honest status",
        "Every page says what works today and what does not exist yet. If something is a \
         placeholder, it is labelled as one.",
    ),
    (
        "Upstreams respected",
        "Ferrum builds on open-source work. Upstream projects are credited by name and their \
         licenses are followed.",
    ),
];

const CREDITS: &[(&str, &str)] = &[
    (
        "Zed",
        "The open-source editor GPUI and the ui and theme crates come from, developed by Zed \
         Industries.",
    ),
    (
        "GPUI",
        "Zed's GPU-accelerated UI framework. Every Ferrum desktop app renders with it, \
         together with Zed's ui and theme crates.",
    ),
    (
        "SQLx",
        "Async SQL toolkit powering FerrumData's PostgreSQL support.",
    ),
    (
        "alacritty_terminal",
        "Terminal emulation behind FerrumTerm, from the Alacritty project.",
    ),
    (
        "reqwest & rustls",
        "HTTP and TLS for FerrumAPI and Ferrum Launcher.",
    ),
    (
        "Axum & maud",
        "The Rust web server and HTML templating that render this site.",
    ),
];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("About"),
        "Ferrum is a family of native, local-first developer tools written in Rust, without \
         Electron. Our principles, the status of each tool and credits.",
        "/about",
        "page-about",
    );
    page(
        site_url,
        &meta,
        html! {
            section class="about-hero" aria-labelledby="about-title" {
                div class="container" {
                    p class="eyebrow" { "About Ferrum" }
                    h1 id="about-title" { "Developer tools, forged in Rust" }
                    p class="lead" {
                        "Ferrum is a family of native developer tools written in Rust: a code \
                         editor, an API client, a Git client, a database IDE, a terminal, a \
                         container manager and developer utilities, plus Ferrum Launcher to install and update them. They are local-first, \
                         share one GPUI-based interface and run without Electron."
                    }
                }
            }
            section class="section" aria-labelledby="principles-title" {
                div class="container" {
                    p class="eyebrow" { "Principles" }
                    h2 id="principles-title" { "What we hold to" }
                    ol class="about-principles grid grid--2" {
                        @for (title, body) in PRINCIPLES {
                            li class="card" {
                                h3 { (title) }
                                p class="muted" { (body) }
                            }
                        }
                    }
                }
            }
            section class="section" aria-labelledby="status-title" {
                div class="container" {
                    p class="eyebrow" { "Status" }
                    h2 id="status-title" { "Where each tool stands" }
                    ul class="about-status" {
                        @for p in products::ALL {
                            li class="about-status__row" {
                                (card::app_logo(&p, "app-logo--xs", true))
                                span class="about-status__name" {
                                    a href=(p.path()) { (p.name) }
                                    span class="muted" { " · " (p.kind) }
                                }
                                (badge::status(p.status))
                            }
                        }
                    }
                    p class="muted" {
                        "Builds are free public downloads, installed through Ferrum Launcher. "
                        a href=(site::DOWNLOAD_PATH) { "Download it" } "."
                    }
                }
            }
            section class="section" aria-labelledby="credits-title" {
                div class="container" {
                    p class="eyebrow" { "Credits" }
                    h2 id="credits-title" { "Built on the shoulders of" }
                    dl class="spec-sheet" {
                        @for (name, what) in CREDITS {
                            div class="spec-sheet__row" {
                                dt class="mono" { (name) }
                                dd { (what) }
                            }
                        }
                    }
                    p class="muted" {
                        "GPUI and the ui and theme crates come from the open-source "
                        a href=(site::ZED_UPSTREAM_URL) rel="noopener" { "Zed editor" }
                        ". Ferrum is not affiliated with or endorsed by Zed Industries."
                    }
                }
            }
            section class="section section--tight" aria-labelledby="contact-title" {
                div class="container" {
                    h2 id="contact-title" { "Contact" }
                    @match site::CONTACT_EMAIL {
                        Some(email) => p { a href={ "mailto:" (email) } { (email) } },
                        None => p class="muted" { "Contact details coming soon." },
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
    fn renders_single_h1_principles_and_credits() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("Local-first"));
        assert!(html.contains("GPUI"));
        assert!(html.contains(site::ZED_UPSTREAM_URL));
    }

    #[test]
    fn lists_every_product_with_status() {
        let html = render("https://example.test").into_string();
        for p in products::ALL {
            assert!(html.contains(&format!("href=\"{}\"", p.path())));
        }
        assert!(html.contains("Alpha"));
        assert!(html.contains("Alpha"));
    }

    #[test]
    fn contact_falls_back_when_unset() {
        let html = render("https://example.test").into_string();
        if site::CONTACT_EMAIL.is_none() {
            assert!(html.contains("Contact details coming soon"));
        }
    }
}

//! `/faq`: short answers, each backed by the READMEs, INSTALL.md, the
//! privacy page or `site.rs`. The same text feeds the `FAQPage` JSON-LD.

use maud::{Markup, html};

use crate::components::{breadcrumb::Crumb, cta, page_header::PageHeader};
use crate::layout::{PageMeta, page};
use crate::{seo, site};

pub const PATH: &str = "/faq";

/// A question, its plain-text answer, and an optional "read more" link.
struct Faq {
    q: &'static str,
    a: String,
    more: Option<(&'static str, &'static str)>,
}

fn signing_answer() -> String {
    if site::MACOS_NOTARIZED {
        format!(
            "Yes. macOS releases are signed with the {} {} Developer ID and notarized by Apple, so \
             they open without a warning. The launcher also checks every download against its \
             published SHA-256 checksum before installing it.",
            site::ORGANIZATION_LEGAL_NAME,
            site::ORGANIZATION_CITY
        )
    } else {
        "Not yet. Current macOS builds are ad-hoc signed but not notarized, so a downloaded DMG \
         needs Open Anyway once. The launcher checks every download against its published \
         SHA-256 checksum before installing it."
            .to_owned()
    }
}

fn faqs() -> Vec<Faq> {
    vec![
        Faq {
            q: "What is Ferrum?",
            a: "A family of native developer tools for macOS, written in Rust and drawn on the \
                GPU with GPUI: Ferrum Editor, FerrumVibe, FerrumAPI, FerrumGIT, FerrumData, \
                FerrumTerm, FerrumKit, FerrumContainers, FerrumPorts and FerrumEnv. Ferrum \
                Launcher installs and updates them."
                .to_owned(),
            more: Some(("See every app", super::apps::PATH)),
        },
        Faq {
            q: "How much does Ferrum cost?",
            a: "Nothing. Every build is a free public download, with no account, sign-in or \
                license key."
                .to_owned(),
            more: Some(("Download Ferrum Launcher", site::DOWNLOAD_PATH)),
        },
        Faq {
            q: "Is Ferrum ready for daily use?",
            a: "Every app is in alpha: usable, released often, with rough edges. Each app's page \
                lists what works today and what isn't built yet, and the launcher lets you roll \
                back to an earlier version."
                .to_owned(),
            more: Some(("Read the changelog", "/changelog")),
        },
        Faq {
            q: "Which systems does it run on?",
            a: "macOS on Apple Silicon today. Windows and Linux installers for the launcher are \
                coming; FerrumTerm won't be available on Windows at first."
                .to_owned(),
            more: Some(("Install guide", "/docs/install")),
        },
        Faq {
            q: "Do I need an account?",
            a: "No. Builds are public GitHub releases, and the launcher installs every app \
                without signing in. Ferrum Editor's CI page asks for a GitHub token only when \
                you open it, and can reuse the GitHub CLI's. The agent chat in Ferrum Editor and \
                FerrumVibe uses your own API key, a local Ollama or an agent you already \
                have, such as Claude Code."
                .to_owned(),
            more: None,
        },
        Faq {
            q: "Where is my data stored?",
            a: "On your Mac. FerrumAPI keeps collections and history in a local SQLite file, \
                FerrumEnv keeps an encrypted vault in ~/.ferrum/env, and the apps share recent \
                projects in ~/.ferrum/projects.json. There is no Ferrum server that holds your \
                work."
                .to_owned(),
            more: Some(("How Ferrum handles security", "/security")),
        },
        Faq {
            q: "Do the apps collect telemetry?",
            a: "No. The apps contain no analytics or telemetry. They connect to what you use \
                them with: GitHub for releases and the CI page, the model provider or agent you \
                pick for the agent chat, and your own APIs, databases and container engine. This website sets no cookies and runs no analytics."
                .to_owned(),
            more: Some(("Website privacy", "/privacy")),
        },
        Faq {
            q: "Are the builds signed and notarized?",
            a: signing_answer(),
            more: Some(("Security", "/security")),
        },
        Faq {
            q: "How do updates work?",
            a: "Ferrum Launcher checks for new releases on startup and every 6 hours, updates \
                each app with one click and updates itself in place. It keeps the last three \
                versions of each app, so you can switch back."
                .to_owned(),
            more: Some(("Updates and rollback", "/docs/install#updates")),
        },
        Faq {
            q: "Can I open the apps from a terminal?",
            a: "Yes. Turn on a shell command for an app in the launcher, then run fee . to open \
                the current folder in Ferrum Editor, feg . for FerrumGIT or fet for FerrumTerm."
                .to_owned(),
            more: Some(("Shell commands", "/docs/shell-commands")),
        },
        Faq {
            q: "Is Ferrum open source?",
            a: format!(
                "No. The apps are proprietary (© {}, all rights reserved) and their source isn't \
                 published; the public releases repository holds builds only. They include \
                 permissively licensed open-source work: GPUI from the Zed editor (Apache-2.0), \
                 Lucide icons (ISC) and the IBM Plex Sans and Lilex fonts (OFL 1.1).",
                site::ORGANIZATION_LEGAL_NAME
            ),
            more: Some(("Open source licences", "/licenses")),
        },
        Faq {
            q: "Who makes Ferrum?",
            a: format!(
                "{}, a software studio in {}. Ferrum is not affiliated with or endorsed by Zed \
                 Industries.",
                site::ORGANIZATION_LEGAL_NAME,
                site::ORGANIZATION_CITY
            ),
            more: Some(("About Ferrum", "/about")),
        },
    ]
}

pub fn render(site_url: &str) -> Markup {
    let items = faqs();
    let pairs: Vec<(&str, String)> = items.iter().map(|f| (f.q, f.a.clone())).collect();
    let meta = PageMeta::new(
        Some("FAQ: price, platforms, privacy and signing"),
        "Answers about Ferrum: it's free, in alpha, macOS on Apple Silicon today, keeps your \
         data on your Mac, has no telemetry, and ships signed builds.",
        PATH,
        "page-faq",
    )
    .trail(vec![Crumb::new("FAQ", PATH)])
    .json_ld(seo::faq_page(&pairs));
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Frequently asked questions")
                .lead(html! { p { "Short answers about cost, platforms, your data and how builds are made." } })
                .render())
            div class="container" {
                dl class="faq" {
                    @for (i, f) in items.iter().enumerate() {
                        div class="faq__item" id={ "q" (i + 1) } {
                            dt class="faq__q" { (f.q) }
                            dd class="faq__a" {
                                p { (f.a) }
                                @if let Some((label, href)) = f.more {
                                    p class="faq__more" { a href=(href) { (label) } }
                                }
                            }
                        }
                    }
                }
            }
            (cta::get_launcher())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn questions_match_the_structured_data() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(r#""@type":"FAQPage""#));
        for f in faqs() {
            assert!(html.contains(f.q), "{}", f.q);
        }
        assert_eq!(html.contains("notarized by Apple"), site::MACOS_NOTARIZED);
    }
}

//! `/faq`: short answers, each backed by the READMEs, INSTALL.md, the
//! privacy page or `site.rs`. The same text feeds the `FAQPage` JSON-LD.

use maud::{Markup, html};

use crate::components::{breadcrumb::Crumb, cta, page_header::PageHeader};
use crate::layout::{PageMeta, page};
use crate::{seo, site};

pub const PATH: &str = "/faq";

/// The announcement post, linked from the open-source answer.
const OPEN_SOURCE_POST: &str = "/updates/ferrum-is-open-source";

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
            a: "A family of open-source native developer tools for macOS and Linux, written in \
                Rust and drawn on the GPU with GPUI: FerrumIDE, FerrumEngineer, FerrumAPI, FerrumGIT, FerrumData, \
                FerrumTerm, FerrumKit, FerrumContainers, FerrumPorts and FerrumVariables. \
                FerrumLauncher installs and updates them."
                .to_owned(),
            more: Some(("See every app", super::apps::PATH)),
        },
        Faq {
            q: "Is Ferrum free and open source?",
            a: format!(
                "Yes, both. Every build is a free public download, with no account, sign-in or \
                 licence key. Since {} the source of every app and the launcher is public at {} \
                 under the MIT licence or Apache-2.0, whichever you prefer ({}).",
                crate::updates::display_date(site::OPEN_SOURCE_DATE),
                site::SOURCE_REPO_URL.trim_start_matches("https://"),
                site::LICENSE_SPDX
            ),
            more: Some(("Ferrum is open source", OPEN_SOURCE_POST)),
        },
        Faq {
            q: "How can I contribute?",
            a: "Report bugs and suggest features in GitHub issues, and read CONTRIBUTING.md in \
                the repository before you open a pull request. Security problems go to a private \
                GitHub security advisory, not a public issue."
                .to_owned(),
            more: Some(("Ferrum on GitHub", site::SOURCE_REPO_URL)),
        },
        Faq {
            q: "Can I build the apps myself?",
            a: format!(
                "Yes. Clone the repository and run an app with cargo, for example FerrumIDE: {}. \
                 rust-toolchain.toml pins the Rust version, the first build compiles GPUI (a \
                 few minutes), and each app's README names its package.",
                site::BUILD_FROM_SOURCE.join(", then ")
            ),
            more: Some(("Build from source", "/download#source")),
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
            a: "macOS on Apple Silicon and Intel, and Linux on x86_64 (Debian, Ubuntu and \
                derivatives, as a .deb). Windows builds compile but aren't published yet, and \
                FerrumTerm and FerrumEngineer have no Windows build."
                .to_owned(),
            more: Some(("Install guide", "/docs/install")),
        },
        Faq {
            q: "Do I need an account?",
            a: "No. Builds are public GitHub releases, and the launcher installs every app \
                without signing in. FerrumIDE's CI page asks for a GitHub token only when \
                you open it, and can reuse the GitHub CLI's. The agent chat in FerrumIDE and \
                FerrumEngineer uses your own API key, a local Ollama or an agent you already \
                have, such as Claude Code."
                .to_owned(),
            more: None,
        },
        Faq {
            q: "Where is my data stored?",
            a: "On your machine. FerrumAPI keeps collections and history in a local SQLite file, \
                FerrumVariables keeps an encrypted vault in ~/.ferrum/env, and the apps share recent \
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
            a: "FerrumLauncher checks for new releases on startup and every 6 hours, updates \
                each app with one click and updates itself in place. It keeps the last three \
                versions of each app, so you can switch back."
                .to_owned(),
            more: Some(("Updates and rollback", "/docs/install#updates")),
        },
        Faq {
            q: "Can I open the apps from a terminal?",
            a: "Yes. Turn on a shell command for an app in the launcher, then run fei . to open \
                the current folder in FerrumIDE, feg . for FerrumGIT or fet for FerrumTerm."
                .to_owned(),
            more: Some(("Shell commands", "/docs/shell-commands")),
        },
        Faq {
            q: "What else is inside the apps?",
            a: "Permissively licensed open-source work: GPUI from the Zed editor (Apache-2.0), \
                Lucide icons (ISC) and the IBM Plex Sans and Lilex fonts (OFL 1.1). A licence \
                check in CI fails any build that pulls in GPL-family code."
                .to_owned(),
            more: Some(("Licences", "/licenses")),
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
        Some("FAQ: open source, platforms, privacy and signing"),
        "Answers about Ferrum: free and open source (MIT or Apache-2.0), in alpha, for macOS and \
         Linux, keeps your data local, has no telemetry, and ships signed builds.",
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
                .lead(html! { p { "Short answers about the licence, platforms, your data and how builds are made." } })
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
        assert!(html.contains("Is Ferrum free and open source?"));
        assert!(html.contains("MIT OR Apache-2.0"));
        assert!(html.contains(OPEN_SOURCE_POST));
        let lower = html.to_lowercase();
        for gone in ["proprietary", "all rights reserved", "closed source"] {
            assert!(!lower.contains(gone), "{gone}");
        }
    }
}

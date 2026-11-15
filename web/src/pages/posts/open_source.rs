//! "Ferrum is open source" (2026-11-15). Claims follow the monorepo README
//! (what each folder holds), INSTALL.md (releases repo, Windows builds),
//! THIRD_PARTY_NOTICES.md and `scripts/check-licenses.sh` (the licence check)
//! and `site.rs` (repository, licence, build commands).

use maud::{Markup, html};

use crate::products;
use crate::site;
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "ferrum-is-open-source",
    title: "Ferrum is open source",
    date: "2026-11-15",
    byline: "Ferrum Team",
    summary: "The source of every Ferrum app, Ferrum Launcher and this website is now public on \
              GitHub under MIT OR Apache-2.0. What's open, where it lives, how to build it and \
              how to help.",
    body,
};

/// Libraries and tools in the repository besides the apps (monorepo README).
const ALSO_OPEN: [(&str, &str); 5] = [
    (
        "ferrum-ui",
        "the shared shell, theme, widgets, project bar, onboarding and settings every app uses",
    ),
    (
        "FerrumChat",
        "the agent chat inside FerrumIDE and FerrumEngineer",
    ),
    (
        "ferrumlogs-core and ferrumci-core",
        "the libraries behind FerrumIDE's Logs and CI pages",
    ),
    (
        "scripts and the release workflow",
        "how every DMG, .deb and archive is built, signed and published",
    ),
    (
        "web",
        "this website, a Rust server built with axum and maud",
    ),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "As of today, Ferrum's source is public at "
            a href=(site::SOURCE_REPO_URL) rel="noopener" { (site::SOURCE_REPO_SLUG) }
            ". You can read it, build it, change it and ship your own copy."
        }

        h2 id="whats-open" { "What's open" }
        p { "All of it. Every app and the launcher:" }
        ul class="post__list" {
            @for p in products::ALL {
                li { a href=(p.path()) { (p.name) } " (" (p.kind) ")" }
            }
        }
        p { "And what they're built from:" }
        ul class="post__list" {
            @for (name, what) in ALSO_OPEN {
                li { strong { (name) } ": " (what) "." }
            }
        }

        h2 id="licence" { "The licence" }
        p {
            "Ferrum is offered under the MIT licence or Apache-2.0, whichever you prefer ("
            code { (site::LICENSE_SPDX) } "), the pairing most Rust crates use. You can use the \
             code in open or closed projects, commercial or not, as long as you keep the notices."
        }
        p {
            "The third-party work inside the apps keeps its own licence: GPUI from the Zed editor \
             (Apache-2.0), the Lucide icons (ISC) and the IBM Plex Sans and Lilex fonts (OFL 1.1). \
             A licence check in CI still fails any build that pulls in GPL-family code. The full \
             list and texts are on "
            a href=(crate::pages::licenses::PATH) { "Licences" } "."
        }

        h2 id="where" { "Where the code is" }
        p {
            "The source lives in " a href=(site::SOURCE_REPO_URL) rel="noopener" { (site::SOURCE_REPO_SLUG) }
            ". Builds stay where they were, in the public "
            a href=(site::RELEASES_URL) rel="noopener" { (site::REPO_SLUG) } " releases, so \
             Ferrum Launcher keeps installing and updating your apps as before. Nothing changes \
             for anything you have installed."
        }

        h2 id="build" { "Build it" }
        p {
            "With Rust installed (the repository's " code { "rust-toolchain.toml" } " picks the \
             version), three commands build and run FerrumIDE:"
        }
        pre class="code-block" {
            code { @for line in site::BUILD_FROM_SOURCE { (line) "\n" } }
        }
        p {
            "Each app's README names its package and says what works today and what isn't \
             built yet."
        }

        h2 id="contribute" { "How to help" }
        ul class="post__list" {
            li {
                "Report bugs and suggest features in "
                a href=(site::ISSUES_URL) rel="noopener" { "issues" } "."
            }
            li {
                "Ask questions and share what you've built in "
                a href=(site::DISCUSSIONS_URL) rel="noopener" { "discussions" } "."
            }
            li {
                "Before a pull request, read "
                a href=(site::CONTRIBUTING_URL) rel="noopener" { "CONTRIBUTING.md" } "."
            }
            li {
                "Report security problems privately through a "
                a href=(site::SECURITY_ADVISORY_URL) rel="noopener" { "security advisory" }
                ", not a public issue."
            }
        }

        h2 id="next" { "What's next" }
        p {
            "The same as before, in the open. Every app stays in alpha and keeps shipping \
             through the launcher, and every release is on the "
            a href=(crate::pages::changelog::PATH) { "changelog" } ". Windows builds compile \
             in CI but haven't been tested by hand yet, so that's one place where help counts."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_what_is_open_the_licence_and_how_to_build() {
        let html = body().into_string();
        for p in products::ALL {
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
        }
        assert!(html.contains("MIT OR Apache-2.0"));
        for url in [
            site::SOURCE_REPO_URL,
            site::RELEASES_URL,
            site::ISSUES_URL,
            site::CONTRIBUTING_URL,
            site::SECURITY_ADVISORY_URL,
        ] {
            assert!(html.contains(&format!(r#"href="{url}""#)), "{url}");
        }
        for line in site::BUILD_FROM_SOURCE {
            assert!(html.contains(line), "{line}");
        }
        assert_eq!(POST.date, site::OPEN_SOURCE_DATE);
    }
}

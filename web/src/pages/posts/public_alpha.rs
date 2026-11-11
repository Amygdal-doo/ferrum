//! "Ferrum is in public alpha" (2026-11-11). Every claim follows the app
//! READMEs, `catalog.json`, INSTALL.md and the product pages.

use maud::{Markup, html};

use super::product_row;
use crate::pages::{feature_demos, home};
use crate::products::{self, FERRUM_LAUNCHER};
use crate::site;
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "ferrum-public-alpha",
    title: "Ferrum is in public alpha",
    date: "2026-11-11",
    byline: "Ferrum Team",
    summary: "Nine native developer tools written in Rust, plus the launcher that installs \
              them, are a free download for Apple Silicon Macs. What's in the alpha, what to \
              expect from it, and what isn't built yet.",
    body,
};

/// Bigger gaps from each app's "Not built yet" list (product pages).
const NOT_BUILT_YET: [(&str, &str); 9] = [
    (
        "Ferrum Editor",
        "packages and plugins, code folding, multiple windows",
    ),
    (
        "FerrumAPI",
        "OAuth 2.0 and Digest auth, pre-request and test scripts",
    ),
    (
        "FerrumGIT",
        "interactive rebase and a merge-conflict editor",
    ),
    (
        "FerrumData",
        "row editing, syntax highlighting and autocomplete",
    ),
    ("FerrumTerm", "a config file, themes and search"),
    ("FerrumKit", "saved inputs between launches"),
    (
        "FerrumDock",
        "building images and pulling from private registries",
    ),
    (
        "FerrumPorts",
        "column sorting and keyboard navigation in the table",
    ),
    ("FerrumEnv", "unlocking with Touch ID, and per-person keys"),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "Ferrum is a family of developer tools written in Rust: a code editor, an API \
             client, a Git client, a database IDE, a terminal and four more, each a native app \
             drawn on the GPU. All of them are now a free public download for Macs with Apple \
             Silicon, installed through Ferrum Launcher. They are alpha software, and this post \
             says what that means in practice."
        }
        figure class="post__figure" style=(products::FERRUM_EDITOR.accent_style()) {
            div class="post__demo" role="img"
                aria-label="Ferrum Editor with a tree view and tabs: three cursors type at once, then the command palette filters commands" {
                (feature_demos::editor())
            }
            figcaption {
                "Ferrum Editor, the suite's code editor. An animated mock-up of the app, drawn \
                 in HTML for this page."
            }
        }
        p {
            "Each app is compiled Rust rendered with GPUI, the GPU UI framework from the Zed \
             editor. There is no Electron, no webview and no bundled browser runtime. Requests, \
             queries, repositories and terminal sessions stay on your Mac. The apps share one \
             interface and one theme, and they open each other."
        }
        p {
            "There is no Ferrum account. Nothing to sign up for before you use the apps; \
             the CI page in Ferrum Editor and the pull request views in FerrumGIT use your own \
             GitHub or GitLab token."
        }

        h2 id="what-is-available" { "What is available" }
        p {
            "Nine apps and the launcher that installs them. Each name links to the app's \
             page, which lists what works today and what isn't built yet."
        }
        ul class="post-apps" {
            @for p in products::apps() {
                (product_row(p, p.tagline))
            }
        }

        h3 { "Ferrum Launcher installs all of them" }
        ul class="post-apps" {
            (product_row(&FERRUM_LAUNCHER, FERRUM_LAUNCHER.tagline))
        }
        p {
            "You download one thing. The launcher installs any app with one click, checks each \
             download against its published checksum, updates the apps and itself in place, and \
             keeps the last three versions of each app so you can switch back. Releases come \
             from a public GitHub repository, so there is nothing to sign in to."
        }
        (home::install_command())
        p class="post__aside" {
            "Or " a href=(site::DOWNLOAD_PATH) { "download the DMG" } " and drag it to Applications."
        }

        h3 { "One project across the apps" }
        p {
            "Ferrum Editor, FerrumAPI, FerrumGIT, FerrumData, FerrumTerm, FerrumPorts and \
             FerrumEnv share a project bar at the top of the window. It shows the \
             current project's name, path and Git branch. "
            kbd { "⌥⌘O" } " switches between recent projects, a list every app shares, and \
             buttons open the project in Ferrum Editor, FerrumGIT or FerrumTerm."
        }

        h3 { "From your shell" }
        p {
            "Turn on a shell command for an app in the launcher, and open things from any \
             terminal:"
        }
        pre class="code-block" {
            code {
                "fee .   # Ferrum Editor on this folder\n"
                "feg .   # FerrumGIT on this repository\n"
                "fet     # a FerrumTerm session in this folder"
            }
        }
        p {
            "Commands are off by default and you pick their names. The launcher also links each \
             app's own command-line tools, such as " code { "fterm" } ", into "
            code { "~/.ferrum/bin" } "."
        }

        h3 { "Platforms" }
        p {
            "Today Ferrum is for Macs with Apple Silicon. The launcher comes as a DMG"
            @if site::MACOS_NOTARIZED { ", signed and notarized by Apple," }
            " or as a one-line install from Terminal. The release workflow also builds the \
             launcher, and many of the apps, for Windows and Linux on x86_64. Those builds have \
             only been compiled in CI, not tried by hand, so the download page still lists them \
             as coming soon. FerrumTerm has no Windows build."
        }

        h2 id="what-to-expect" { "What to expect" }
        p {
            "Alpha means usable for real work, released often, and rough at the edges. In \
             particular:"
        }
        ul class="post__list" {
            li {
                strong { "Tested, but not all by hand. " }
                "The logic in every app is covered by automated tests, many of them headless \
                 tests that drive the real views. Some windows haven't been clicked through by a \
                 person yet, FerrumEnv and FerrumPorts among them. Each app's page \
                 says how it was tested."
            }
            li {
                strong { "Frequent updates. " }
                "The launcher checks for new releases when it starts and every six hours. If a \
                 release breaks something for you, activate the previous version on the app's \
                 page in the launcher."
            }
            li {
                strong { "Missing pieces. " }
                "Every product page has a \"Not built yet\" list. We would rather show you that \
                 list than write around it."
            }
            li {
                strong { "Local data. " }
                "The apps keep their data on your Mac. FerrumEnv encrypts its vault behind a \
                 master password."
            }
        }

        h2 id="what-it-costs" { "What it costs" }
        p {
            "Ferrum is free to download and use. There is no account, no sign-in and no licence \
             key: the launcher and every app come from "
            a href=(site::RELEASES_URL) rel="noopener" { "public releases on GitHub" } "."
        }

        h2 id="what-is-next" { "What is next" }
        p {
            "No dates here, only the direction. Windows and Linux installers for the launcher \
             are listed as coming soon. Beyond that, these are some of the bigger gaps on each \
             app's \"Not built yet\" list:"
        }
        ul class="post__list" {
            @for (name, items) in NOT_BUILT_YET {
                li { strong { (name) ": " } (items) }
            }
        }
        p {
            "We'll write here when something lands. Follow the "
            a href=(crate::updates::FEED_PATH) { "Atom feed" } " to get it in your feed reader."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn presents_every_product_with_logo_and_link() {
        let html = body().into_string();
        for p in products::ALL {
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
            assert!(
                html.contains(&format!("/assets/{}?v=", p.logo_asset())),
                "{}",
                p.name
            );
        }
        for h2 in [
            "What is available",
            "What to expect",
            "What it costs",
            "What is next",
        ] {
            assert!(html.contains(&format!(">{h2}</h2>")), "{h2}");
        }
        assert!(html.contains("⌥⌘O"));
        assert!(html.contains("fee ."));
        assert!(html.contains("feg ."));
        assert!(html.contains("fet "));
    }

    #[test]
    fn names_no_prices_or_user_counts() {
        let html = body().into_string().to_lowercase();
        for word in ["$", "€", "per month", "pricing", "users", "waitlist"] {
            assert!(!html.contains(word), "{word}");
        }
    }
}

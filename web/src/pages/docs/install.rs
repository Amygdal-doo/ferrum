//! `/docs/install`: installing Ferrum Launcher and, through it, the apps.
//! Follows INSTALL.md at the repo root.

use maud::{Markup, html};

use super::{article, trail};
use crate::components::{
    callout::{self, Kind},
    card, code,
};
use crate::layout::PageMeta;
use crate::{products, site};

pub const PATH: &str = "/docs/install";
pub const TITLE: &str = "Install Ferrum Launcher";

const UNINSTALL_ALL: &str = "rm -rf \"/Applications/Ferrum Launcher.app\" ~/Applications/Ferrum ~/.ferrum \\\n       ~/Library/Application\\ Support/dev.Ferrum.FerrumLauncher";

const FILES: [(&str, &str); 5] = [
    ("Launcher", "/Applications/Ferrum Launcher.app"),
    ("Active apps", "~/Applications/Ferrum/<App>.app"),
    (
        "Installed versions",
        "~/Library/Application Support/dev.Ferrum.FerrumLauncher/apps/<app>/<version>/",
    ),
    (
        "Settings and state",
        "~/Library/Application Support/dev.Ferrum.FerrumLauncher/state.json",
    ),
    ("Command-line links", "~/.ferrum/bin/"),
];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Install Ferrum Launcher on macOS"),
        "Install Ferrum Launcher with one Terminal command or the DMG, then install, update, \
         roll back and uninstall every Ferrum app from it. No account needed.",
        PATH,
        "page-docs",
    )
    .trail(trail("Install", PATH));
    article(
        site_url,
        &meta,
        TITLE,
        html! {
            p {
                "You install Ferrum Launcher once. It installs, updates and rolls back every \
                 other Ferrum app, which have no installers of their own. Builds are public, so \
                 you need no GitHub account."
            }
        },
        html! {
            h2 id="requirements" { "What you need" }
            p {
                "A Mac with Apple Silicon. The launcher downloads builds from the public "
                code { (site::REPO_SLUG) } " repository on GitHub, so it needs to reach GitHub."
            }

            h2 id="terminal" { "Option A: one line in Terminal" }
            (code::command(site::MACOS_INSTALL_COMMAND))
            p {
                "The script picks the newest " code { "launcher-v*" } " release for your CPU, \
                 checks its " code { ".sha256" } ", installs into " code { "/Applications" }
                " (or " code { "~/Applications" } " if that isn't writable) and opens it. \
                 Files downloaded with curl aren't quarantined, so macOS doesn't ask about them."
            }

            h2 id="dmg" { "Option B: the disk image" }
            ol class="prose-steps" {
                li {
                    "Download " code { "FerrumLauncher-<version>-aarch64.dmg" } " with "
                    a href=(site::MACOS_DOWNLOAD_PATH) { "Download for macOS" } ", or pick it \
                     from the newest " code { "launcher-v…" } " release on "
                    a href=(site::RELEASES_URL) rel="noopener" { "GitHub" } "."
                }
                li { "Open it and drag " strong { "Ferrum Launcher" } " onto " strong { "Applications" } "." }
                li { "Open Ferrum Launcher. There is nothing to sign in to." }
            }
            @if site::MACOS_NOTARIZED {
                p {
                    "Releases are signed with the " (site::ORGANIZATION_LEGAL_NAME) " "
                    (site::ORGANIZATION_CITY) " Developer ID and notarized by Apple, so macOS \
                     opens them without a warning."
                }
            } @else {
                (callout::render(Kind::Warn, Some("First launch"), html! {
                    p {
                        "This build isn't notarized yet. Use " strong { "System Settings → \
                         Privacy & Security → Open Anyway" } " once, or install with the \
                         Terminal command above."
                    }
                }))
            }

            h2 id="apps" { "Install the apps" }
            p {
                "Click an app in the launcher's overview, then " strong { "Install" } ". The \
                 launcher downloads the build with a progress bar you can cancel, checks it \
                 against the published " code { ".sha256" } " and refuses to install on a \
                 mismatch. It then publishes the active version as "
                code { "~/Applications/Ferrum/<App>.app" } ", so Launchpad and Spotlight find \
                 it, and links its executables into " code { "~/.ferrum/bin" } "."
            }
            p { "To run those executables from any shell, add the folder to your PATH:" }
            (code::block("echo 'export PATH=\"$HOME/.ferrum/bin:$PATH\"' >> ~/.zshrc"))
            p {
                "Turning on a " a href=(super::shell_commands::PATH) { "shell command" }
                " in the launcher does this for you."
            }
            (card::app_strip(products::apps(), false))

            h2 id="updates" { "Updates and rollback" }
            ul class="prose-list" {
                li {
                    "The launcher checks for new releases on startup and every 6 hours (change \
                     it in Settings). Apps with a newer release get an "
                    strong { "Update to vX" } " button."
                }
                li {
                    "The launcher updates itself in place: " strong { "Update Launcher to vX" }
                    " downloads the new version, replaces the app and restarts it."
                }
                li {
                    "To roll back, pick an older version in an app's version list and click "
                    strong { "Activate" } "."
                }
                li {
                    "The last 3 versions of each app are kept (Settings → Versions to keep; 0 \
                     keeps all). The active version is never removed."
                }
                li {
                    "Every app follows the " strong { "Alpha" } " channel by default. Switch an \
                     app to Beta or Stable on its page."
                }
            }

            h2 id="uninstall" { "Uninstall" }
            p {
                "Remove one version with " strong { "Remove" } " in its row. Removing an app's \
                 last version also deletes its app bundle and command-line links. To remove \
                 everything:"
            }
            (code::block(UNINSTALL_ALL))

            h2 id="files" { "Where files live" }
            dl class="spec-sheet" {
                @for (what, path) in FILES {
                    div class="spec-sheet__row" {
                        dt { (what) }
                        dd { code { (path) } }
                    }
                }
            }
            p class="muted" {
                "Windows and Linux installers are listed on the "
                a href=(site::DOWNLOAD_PATH) { "download page" } " once they ship."
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shows_the_install_command_and_dmg() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains("install.sh | bash"));
        assert!(html.contains("FerrumLauncher-&lt;version&gt;-aarch64.dmg"));
        assert!(html.contains("~/.ferrum/bin"));
    }
}

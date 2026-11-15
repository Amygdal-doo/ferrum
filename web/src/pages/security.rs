//! `/security`: how builds are delivered and how the apps keep secrets.
//! Every claim follows env/README.md ("Security model"), launcher/README.md
//! ("Install safety", "Authentication"), INSTALL.md, api/README.md and
//! editor/README.md, including the limits they state.

use maud::{Markup, html};

use crate::components::{
    breadcrumb::Crumb,
    callout::{self, Kind},
    cta,
    page_header::PageHeader,
    section::Section,
};
use crate::layout::{PageMeta, page};
use crate::{products, site};

pub const PATH: &str = "/security";

const VAULT: [(&str, &str); 6] = [
    (
        "Key derivation",
        "Argon2id from your master password and a random 16-byte salt: 64 MiB of memory, 3 \
         iterations, 1 lane, giving a 256-bit key. The parameters are stored with the vault, so \
         they can be raised later, and are bounds-checked so a tampered file can't make \
         unlocking eat memory.",
    ),
    (
        "Encryption",
        "XChaCha20-Poly1305 with a fresh random 24-byte nonce on every save. The file header \
         (format, version, key-derivation settings, salt, cipher) is authenticated too, so any \
         change to it or to the ciphertext fails to decrypt.",
    ),
    (
        "On disk",
        "~/.ferrum/env/vault.json. Only the header is readable, not even project names. Saves \
         are atomic (temp file, fsync, rename), mode 0600 in a 0700 folder, and the previous \
         file is kept as an encrypted backup.",
    ),
    (
        "Locking",
        "The master password is asked on every start. The vault locks after 30 minutes without \
         activity (5, 15, 30 or 60 in Settings) or at once with ⌘L, and locking drops the \
         decrypted data and the session key.",
    ),
    (
        "Clipboard",
        "A copied value is cleared from the clipboard after 30 seconds if it's still there, and \
         at once when the vault locks.",
    ),
    (
        "In memory",
        "Variables, share keys, the session key and decrypted buffers are wiped when dropped, \
         and values never appear in logs or debug output.",
    ),
];

const VAULT_LIMITS: [&str; 4] = [
    "Wiping memory is best effort: some copies (text fields, text layout) are freed but not \
     zeroed, and swap and core dumps aren't prevented.",
    "Clipboard managers may keep a copy of a value while it's on the clipboard, and exported \
     .env files are plain text.",
    "Malware running as your user (keyloggers, memory readers) is out of scope.",
    "There is no recovery: a forgotten master password means the data is gone.",
];

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Security: signed builds and the FerrumVariables vault"),
        "Signed builds checked against SHA-256, no telemetry, and FerrumVariables's Argon2id and \
         XChaCha20-Poly1305 vault, with its limits stated plainly.",
        PATH,
        "page-security",
    )
    .trail(vec![Crumb::new("Security", PATH)]);
    let env = products::FERRUMVARIABLES;
    page(
        site_url,
        &meta,
        html! {
            (PageHeader::new(&meta.trail, "Security")
                .lead(html! {
                    p {
                        "How builds reach your Mac, where the apps keep secrets, and what they \
                         don't protect against. Everything here describes the current alpha \
                         builds."
                    }
                })
                .render())

            (Section::new("builds", "Builds and updates").render(html! {
                div class="prose" {
                    ul class="prose-list" {
                        @if site::MACOS_NOTARIZED {
                            li {
                                "macOS releases are signed with the " (site::ORGANIZATION_LEGAL_NAME)
                                " " (site::ORGANIZATION_CITY) " Developer ID, with the hardened \
                                 runtime, and notarized by Apple."
                            }
                        } @else {
                            li { "macOS builds are ad-hoc signed; notarized builds are on the way." }
                        }
                        li {
                            "Every build is a public GitHub release with a " code { ".sha256" }
                            " file. The launcher and the install script check each download \
                             against it, and a mismatch stops the install."
                        }
                        li {
                            "Archives are unpacked into a staging folder and moved into place in \
                             one step, so a failed or cancelled install leaves nothing behind. \
                             Entries with absolute paths, " code { ".." } " or links that point \
                             outside the install folder stop the install."
                        }
                        li {
                            "The public releases need no sign-in. If you point the launcher at a \
                             private repository, its GitHub token goes into the macOS Keychain, \
                             never into a file or a log."
                        }
                        li {
                            "The apps link only permissively licensed third-party code, and CI \
                             fails a build if a GPL-family crate gets in. Each component and its \
                             licence is listed on "
                            a href=(crate::pages::licenses::PATH) { "Licences" } "."
                        }
                    }
                }
            }))

            (Section::new("vault", "The FerrumVariables vault")
                .band()
                .lead(html! {
                    p {
                        a href=(env.path()) { "FerrumVariables" } " keeps your projects' environment \
                         variables behind a master password and shares them encrypted."
                    }
                })
                .render(html! {
                    dl class="spec-sheet" {
                        @for (what, how) in VAULT {
                            div class="spec-sheet__row" { dt { (what) } dd { (how) } }
                        }
                    }
                    h3 { "Sharing between developers" }
                    div class="prose" {
                        p {
                            "Each project has a share key: 256 random bits shown as "
                            code { "fek1_…" } " with a checksum, so typos and truncated pastes are \
                             rejected, and a short fingerprint to compare out loud. Bundles of \
                             environments are sealed with XChaCha20-Poly1305 under that key, with \
                             the project name authenticated. The key itself is never in a bundle."
                        }
                        p {
                            "Anyone with the share key can read and create bundles for that \
                             project: exchange it once, out of band, and rotate it to cut someone \
                             off from future bundles."
                        }
                    }
                    (callout::render(Kind::Security, Some("Limits, honestly"), html! {
                        ul class="prose-list" { @for l in VAULT_LIMITS { li { (l) } } }
                    }))
                }))

            (Section::new("apps", "Secrets in the other apps").render(html! {
                div class="prose" {
                    ul class="prose-list" {
                        li {
                            "FerrumAPI stores collections, environments and their secrets in a \
                             local SQLite file and never logs them. Keychain storage isn't built \
                             yet. Its mock server and webhook inspector listen on "
                            code { "127.0.0.1" } " only."
                        }
                        li {
                            "FerrumIDE's AI agent sessions take API keys through per-agent \
                             environment variables, masked in the panel and never logged."
                        }
                        li {
                            "FerrumIDE's CI page reuses a GitHub token you already have "
                            "(FerrumGIT's Keychain entry or the GitHub CLI), or one you paste."
                        }
                    }
                }
            }))

            (Section::new("privacy", "No telemetry").band().render(html! {
                div class="prose" {
                    p {
                        "The apps contain no analytics or telemetry. They connect to what you use \
                         them with: GitHub for releases and the CI page, and your own APIs, \
                         databases and container engine. This website sets no cookies and runs no \
                         analytics; see "
                        a href="/privacy" { "Privacy" } "."
                    }
                }
            }))

            (Section::new("report", "Report a vulnerability").tight().render(html! {
                div class="prose" {
                    p {
                        "Found a security issue? Report it privately through a "
                        a href=(site::SECURITY_ADVISORY_URL) rel="noopener" {
                            "GitHub security advisory"
                        }
                        " on the Ferrum repository, not in a public issue. Include the app, its \
                         version and the steps to reproduce."
                    }
                    p {
                        "Ferrum is open source, so you can read the code behind every claim on \
                         this page in "
                        a href=(site::SOURCE_REPO_URL) rel="noopener" { (site::SOURCE_REPO_SLUG) } "."
                    }
                }
            }))
            (cta::get_launcher())
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_vault_crypto_and_its_limits() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        for claim in [
            "Argon2id",
            "XChaCha20-Poly1305",
            ".sha256",
            "no analytics",
            "no recovery",
            "security/advisories/new",
        ] {
            assert!(html.contains(claim), "{claim}");
        }
        assert_eq!(html.contains("notarized by Apple"), site::MACOS_NOTARIZED);
    }
}

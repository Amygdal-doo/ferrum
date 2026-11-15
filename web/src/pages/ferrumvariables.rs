//! FerrumVariables product page. Claims follow the FerrumVariables README ("What it
//! does", "Security model", "Not built yet") and nothing beyond it. The
//! security section must keep the README's limits next to its protections.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMVARIABLES;

const FEATURES: &[(&str, &str)] = &[
    (
        "Locked by default",
        "The master password is asked on every start, and nothing is decrypted before. The \
         vault locks after 30 minutes without activity (or 5, 15 or 60), and ⌘L locks it at \
         once.",
    ),
    (
        "Projects and environments",
        "A project links to a folder and starts with development, staging and production. \
         Each variable has a key, a value, an optional comment and a secret flag.",
    ),
    (
        "Secrets stay masked",
        "Reveal one value or all of them. A copied value is cleared from the clipboard after \
         30 s if it's still there, and at once when the vault locks.",
    ),
    (
        "Encrypted bundles",
        "Share all or chosen environments as encrypted text or a .fenv file. Before an \
         import, you see what will be added, changed or removed, by key name only.",
    ),
    (
        "Share keys you can check",
        "Each project has a 256-bit key shown as fek1_…, with a checksum that rejects typos \
         and a short fingerprint to compare out loud. Rotate it to cut someone off from \
         future bundles.",
    ),
    (
        ".env in and out",
        "Import .env, .env.local, .env.production and the rest from a project folder. Write \
         an environment back to a .env file with mode 0600, copy it as text, or compare two \
         environments.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Master password, auto-lock and ⌘L",
    "Projects, environments and variables, with search",
    "Masked secrets, reveal, and copy with clipboard clearing",
    "Share keys, and encrypted bundles as text or .fenv files",
    ".env import and export, and comparing two environments",
    "Changing the master password in Settings",
];

const NOT_BUILT_YET: &[&str] = &[
    "Undo and per-variable history",
    "A command-line tool that runs a command with the variables set",
    "Unlocking with Touch ID or the OS keychain",
    "Per-person public keys instead of one shared key per project",
    "Drag and drop of .env and .fenv files, and opening .fenv files from Finder",
];

/// What the encryption does, in the README's terms.
const PROTECTS: &[&str] = &[
    "The vault key comes from your master password through Argon2id (64 MiB, 3 passes) and \
     a random salt.",
    "The whole vault, project names included, is sealed with XChaCha20-Poly1305 under a \
     fresh nonce on every save. Only the file header is readable on disk.",
    "A wrong password, a modified file or a newer format is reported as an error, not a \
     crash.",
    "Saves are atomic, with mode 0600 in a 0700 folder. The previous file is kept as an \
     encrypted backup.",
    "Bundles are sealed with the project's share key and bound to the project name. The key \
     itself is never inside a bundle.",
    "Values and keys are wiped from memory when they're dropped and are never logged. \
     Locking drops the whole unlocked state.",
];

/// The README's "Limits, honestly", unsoftened.
const LIMITS: &[&str] = &[
    "Wiping memory is best effort: some copies (JSON parsing, text fields, text layout) are \
     freed but not zeroed. Swap and core dumps are not prevented.",
    "A copied value is outside the app's control while it's on the clipboard, and clipboard \
     managers may keep it. Exported .env files are plain text.",
    "There is no protection against malware running as your user, such as keyloggers or \
     memory readers.",
    "Anyone with a project's share key can read and create bundles for that project. \
     Rotating the key only affects future bundles.",
    "There is no recovery: if you forget the master password, the data is gone.",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMVARIABLES,
    meta_description: "FerrumVariables is a native vault for project environment variables: \
                       locked by a master password, shared as encrypted bundles, with .env \
                       import and export.",
    intro: "Your projects' environment variables in one encrypted file instead of loose .env \
            files. Share them with a teammate as an encrypted bundle, sealed with a key you \
            exchange once.",
    features_lead: "The encryption, bundles and .env parsing are unit-tested; the window is \
                    covered by headless tests.",
    features: FEATURES,
    screenshots: &[
        "a project's variables with secret values masked",
        "copying a revealed value",
    ],
    status_summary: "FerrumVariables is in alpha. Its tests are automated; the app hasn't been \
                     clicked through by hand yet.",
    status_groups: &[
        BulletGroup {
            heading: "Works today",
            items: WORKS_TODAY,
        },
        BulletGroup {
            heading: "Not built yet",
            items: NOT_BUILT_YET,
        },
    ],
    specs: &[
        ("Install", "FerrumLauncher (free, see Download)"),
        ("Shell command", "fev, turned on in FerrumLauncher"),
        ("Vault file", "~/.ferrum/env/vault.json, encrypted"),
        (
            "Built with",
            "Rust, GPUI, Argon2id, XChaCha20-Poly1305, zeroize",
        ),
        (
            "Tested with",
            "Automated unit tests and headless window tests, not yet by hand",
        ),
    ],
    specs_note: "A forgotten master password can't be reset. Keep it somewhere safe.",
};

/// Security model: protections and limits side by side.
fn security() -> Markup {
    html! {
        section class="section" aria-labelledby="security-title" {
            div class="container" {
                h2 id="security-title" { "Security model" }
                p class="lead" {
                    "What the encryption protects, and what it can't. Both lists come from \
                     FerrumVariables's own documentation. The "
                    a href="/security#vault" { "security page" }
                    " has the details."
                }
                div class="grid grid--2" {
                    div class="card" {
                        h3 { "How the vault is protected" }
                        ul class="product-list" {
                            @for item in PROTECTS { li { (item) } }
                        }
                    }
                    div class="card" {
                        h3 { "Limits" }
                        ul class="product-list" {
                            @for item in LIMITS { li { (item) } }
                        }
                    }
                }
            }
        }
    }
}

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, security())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_status_and_demo() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">FerrumVariables</h1>"));
        assert!(html.contains("Animated preview of FerrumVariables"));
        assert!(html.contains("/assets/img/brand/env.svg"));
        assert!(html.contains("href=\"/download\""));
    }

    #[test]
    fn security_model_states_its_limits() {
        let html = render("https://example.test").into_string();
        assert!(html.contains("Security model"));
        assert!(html.contains("XChaCha20-Poly1305"));
        assert!(html.contains("best effort"));
        assert!(html.contains("no protection against malware"));
        assert!(html.contains("There is no recovery"));
    }
}

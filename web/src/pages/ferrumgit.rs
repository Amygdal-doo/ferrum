//! FerrumGIT product page. Claims follow the FerrumGIT README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMGIT;

const FEATURES: &[(&str, &str)] = &[
    (
        "Commit graph",
        "Lane layout with merges and branch-offs, ref badges and a WIP row, virtualized \
         for long histories with \"load more\".",
    ),
    (
        "Stage by hunk or line",
        "Stage, unstage or discard whole files, single hunks or just the lines you select \
         in the diff viewer.",
    ),
    (
        "Commit and amend",
        "Summary and description, amend with the last message pre-filled, Cmd+Enter to commit.",
    ),
    (
        "Branches and commits",
        "Create, checkout, rename, delete, merge and rebase branches; cherry-pick, revert, \
         reset and tag commits.",
    ),
    (
        "Conflict flow",
        "A banner for merges, rebases, cherry-picks and reverts in progress, with take ours or \
         theirs, mark resolved, continue and abort.",
    ),
    (
        "Stash, tags and remotes",
        "Save, apply, pop and drop stashes; lightweight and annotated tags; fetch, pull \
         (ff-only, merge or rebase) and push with force-with-lease.",
    ),
    (
        "Per-project SSH profiles",
        "Each repository gets a commit identity and SSH key. Passphrases stay in the OS \
         keychain; generate keys and test connections in the app.",
    ),
    (
        "Uses your git",
        "Runs the system git binary non-interactively, so your git config, hooks and \
         credential helpers keep working.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Open, clone (cancellable) and init; repository tabs restored after restart",
    "Commit graph with lanes, ref badges and a WIP row",
    "Stage, unstage and discard per file, hunk and selected lines",
    "Branches, commits, tags, stash and remotes",
    "Merge, rebase, cherry-pick and revert conflict flow",
    "Profiles with per-repo identity, SSH key and keychain passphrase",
    "File history (follows renames), blame and side-by-side diffs",
    "Commit search and Undo for the last commit, amend or checkout",
    "GitHub and GitLab pull requests: list, review, checkout, create, comment, merge",
];

const NOT_BUILT_YET: &[&str] = &[
    "Redo, and undo for other operations",
    "Interactive rebase",
    "Submodules, LFS and worktrees",
    "Inline review comments, issues, and pull requests from forks",
    "Syntax highlighting in diffs",
    "A merge-conflict editor",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMGIT,
    meta_description: "FerrumGIT is a native, local-first Git client written in Rust with GPUI: \
                       commit graph, hunk and line staging, conflict flow and per-project SSH profiles.",
    intro: "A GitKraken- and Tower-style Git client written in Rust. A native GPUI app on top \
            of the system git binary and SQLite: no backend, no account, no Electron, no webview.",
    features_lead: "FerrumGIT's first slice is tested against the real git binary with temporary \
                    repositories and a local bare remote. This is what it does today.",
    features: FEATURES,
    screenshots: &[
        "Commit graph with branches, tags and the working-copy row",
        "Diff viewer with hunk and line staging",
    ],
    status_summary: "FerrumGIT is in alpha. Git operations are covered by automated \
                     tests; the GPUI interface has been launch-tested on a demo repository but \
                     not yet clicked through by a person.",
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
        ("Requires", "git 2.32 or newer and OpenSSH on PATH"),
        ("Builds", "macOS (Apple Silicon and Intel) and Linux x86_64"),
        ("Built with", "Rust, GPUI, the system git binary, SQLite"),
        (
            "Data",
            "Repository paths and profiles stay local; keys are never copied",
        ),
    ],
    specs_note: "Builds are free public GitHub Releases, installed through \
                 FerrumLauncher.",
};

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, html! {})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rendered() -> String {
        render("https://example.test").into_string()
    }

    #[test]
    fn renders_single_h1_and_alpha_status() {
        let html = rendered();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">FerrumGIT</h1>"));
        assert!(html.contains("Alpha"));
    }

    #[test]
    fn lists_missing_features_and_links_download() {
        let html = rendered();
        assert!(html.contains("Interactive rebase"));
        assert!(html.contains("href=\"/download\""));
        assert!(html.contains("/assets/img/brand/git.svg"));
    }
}

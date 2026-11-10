//! FerrumTerm product page. Claims follow the FerrumTerm README
//! ("What works today" / "Not built yet") and nothing beyond it.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMTERM;

const FEATURES: &[(&str, &str)] = &[
    (
        "Terminals live in a server",
        "Sessions, tabs and split panes run in a background server. Closing the window only \
         disconnects; your programs keep running.",
    ),
    (
        "GPU window",
        "Tabs, splits, truecolor, wide CJK and emoji cells, scrollback and IME input, rendered \
         with GPUI.",
    ),
    (
        "The fterm CLI",
        "Create sessions, run commands in new splits or tabs, send keys, capture output and \
         wait on status, with JSON output for scripts.",
    ),
    (
        "Built for AI agents",
        "Programs report working, done, blocked or error status; you get notifications, and \
         the Mac stays awake while agents work.",
    ),
    (
        "tmux-style attach",
        "fterm attach renders splits and a status line in any terminal, over SSH too, with a \
         C-b prefix for the usual commands.",
    ),
    (
        "Reach it from another Mac",
        "Optional listeners on loopback or your Tailscale address only, protected by a \
         256-bit token.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "Persistent server with sessions, tabs, splits and 10k-line scrollback",
    "fterm server start, stop, status, logs and a login LaunchAgent",
    "Scriptable CLI with --json and status-aware wait",
    "Program status via OSC 7501 or fterm status, with notifications",
    "tmux-style attach",
    "Token-protected remote access over loopback or Tailscale",
    "Sidebar with sessions and an Agents section (Claude Code, Codex)",
    "Mouse selection and copy (⌘C)",
    "Saved SSH hosts and ~/.ssh/config hosts, with fterm ssh",
    "Local, remote and SOCKS port forwards run by the server",
    "What's using a port, with a confirmed kill (fterm port)",
];

const NOT_BUILT_YET: &[&str] = &[
    "Mouse reporting to programs",
    "OSC 8 hyperlinks and ligatures",
    "Config file, themes and search",
    "Restoring sessions after the server itself restarts",
    "Kitty keyboard and graphics protocols",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMTERM,
    meta_description: "FerrumTerm is a Rust terminal for humans and AI agents: a GPUI window, \
                       persistent server-side sessions, the fterm CLI and tmux-style attach.",
    intro: "A terminal written entirely in Rust, inspired by GPU terminals and tmux. A server \
            owns the sessions, and the GPUI window, the fterm CLI and attach are just clients.",
    features_lead: "The server, CLI and attach are covered by automated tests with real PTYs. \
                    The window was recently moved to GPUI.",
    features: FEATURES,
    screenshots: &[
        "FerrumTerm window with tabs and split panes",
        "fterm attach running inside another terminal",
    ],
    status_summary: "FerrumTerm is in alpha. The GPUI window has been launch-tested \
                     (connects, creates a session, sizes the grid) but not yet used day to day.",
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
        ("Install", "Ferrum Launcher (free, see Download)"),
        ("Includes", "The FerrumTerm window and the fterm CLI"),
        ("Builds", "macOS (Apple Silicon and Intel) and Linux x86_64"),
        (
            "Built with",
            "Rust, GPUI, alacritty_terminal, portable-pty, Tokio",
        ),
        ("Tested on", "macOS"),
    ],
    specs_note: "Anyone holding a remote-access token can run programs as you: treat it like \
                 an SSH key.",
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
        assert!(html.contains(">FerrumTerm</h1>"));
        assert!(html.contains("Alpha"));
    }

    #[test]
    fn mentions_the_cli_and_links_download() {
        let html = rendered();
        assert!(html.contains("fterm"));
        assert!(html.contains("href=\"/download\""));
        assert!(html.contains("/assets/img/brand/term.svg"));
    }
}

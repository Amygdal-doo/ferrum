//! FerrumEngineer product page. Claims follow the FerrumEngineer README ("What
//! works" / "Not built yet"), the FerrumChat README (`agent/README.md`) for
//! the chat column, and the `vibe` entry in catalog.json.

use maud::{Markup, html};

use super::product_sections::{self as sections, BulletGroup, ProductPage};
use crate::products::FERRUMENGINEER;

const FEATURES: &[(&str, &str)] = &[
    (
        "Three columns, one project",
        "FerrumChat, FerrumGIT and FerrumTerm side by side on the same folder. Each column is \
         the suite's own view, the same one FerrumIDE shows in its panes.",
    ),
    (
        "Your layout",
        "Close any column and bring it back with ⌥⌘1, ⌥⌘2 or ⌥⌘3. Drag a splitter to resize, \
         or a header to reorder. Order, columns and widths are saved per project.",
    ),
    (
        "Chat with any model or agent",
        "Anthropic, OpenAI (and OpenAI-compatible servers) or a local Ollama, or an ACP agent \
         such as Claude Code, Codex or Gemini CLI. Ask, Agent and Plan modes.",
    ),
    (
        "Review every change",
        "Agent edits go through a review: accept or reject them per hunk, per file or all at \
         once. Each turn is a checkpoint you can restore.",
    ),
    (
        "Columns that talk to each other",
        "Run in terminal runs a command from the chat in the terminal column. Agent edits \
         refresh the git column right away, and files open in FerrumIDE at the line.",
    ),
    (
        "Tools and rules",
        "MCP servers over stdio or HTTP from ~/.ferrum/agent/mcp.json. AGENTS.md, CLAUDE.md, \
         .ferrum/rules and .cursor/rules load on their own.",
    ),
    (
        "Keys in the keychain",
        "API keys are stored in the system keychain, or read from ANTHROPIC_API_KEY and \
         OPENAI_API_KEY. They are never written to a file.",
    ),
];

const WORKS_TODAY: &[&str] = &[
    "FerrumChat, FerrumGIT and FerrumTerm in three columns, built from the shared views",
    "Close, reopen (⌥⌘1, ⌥⌘2, ⌥⌘3), resize and reorder columns",
    "Layout saved per project in ~/.ferrum/vibe/layouts.json",
    "Shared project bar with the ⌥⌘O switcher; switching moves all three columns",
    "ferrumvibe <folder>, or fen from a shell once the launcher command is on",
    "Run in terminal from the chat; agent edits reload the git column",
    "Open file from the chat or a diff opens it in FerrumIDE",
    "Terminals live in the fterm server, so they survive closing the window",
    "Your login shell's PATH, so npx, gemini and MCP servers are found from the Dock",
];

const NOT_BUILT_YET: &[&str] = &[
    "An editor column: files open in a separate FerrumIDE window",
    "More than one chat per window",
    "A Windows build (FerrumTerm's server uses Unix sockets)",
    "Menu bar entries beyond Quit",
    "Prompt caching, images in prompts, MCP OAuth and ACP sign-in",
    "Checkpoints that survive a restart",
];

const PAGE: ProductPage = ProductPage {
    product: FERRUMENGINEER,
    meta_description: "FerrumEngineer puts FerrumChat, FerrumGIT and FerrumTerm side by side on \
                       one project: an agent chat with change review, Git and a terminal in \
                       one native Rust window.",
    intro: "A window for working with coding agents. Talk to a model or an agent in one column, \
            watch its changes in Git in the next, and run things in a terminal in the third.",
    features_lead: "The layout model and the whole window are covered by headless tests with \
                    the three real views.",
    features: FEATURES,
    screenshots: &[
        "the chat editing a file",
        "the change in the git column",
        "the tests running in the terminal column",
    ],
    status_summary: "FerrumEngineer is new in this alpha. Its wiring is tested headlessly, and \
                     the chat's providers are tested against recorded streams: live calls to \
                     Anthropic, OpenAI, Ollama and real ACP agents weren't part of the tests.",
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
        ("Shell command", "fen (turn it on in the launcher)"),
        (
            "For the chat",
            "An Anthropic or OpenAI key, a local Ollama, or an ACP agent such as Claude Code, \
             Codex or Gemini CLI",
        ),
        (
            "Builds",
            "macOS (Apple Silicon and Intel) and Linux x86_64. No Windows build",
        ),
        (
            "Built with",
            "Rust, GPUI, the FerrumChat, FerrumGIT and FerrumTerm views",
        ),
        ("Tested on", "macOS (automated tests)"),
    ],
    specs_note: "Writes, commands and MCP tools ask before they run. Built-in tools can't reach \
                 outside the project folder or write into .git.",
};

pub fn render(site_url: &str) -> Markup {
    sections::render(site_url, &PAGE, html! {})
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_title_status_and_demo() {
        let html = render("https://example.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        assert!(html.contains(">FerrumEngineer</h1>"));
        assert!(html.contains("Animated preview of FerrumEngineer"));
        assert!(html.contains("/assets/img/brand/vibe.svg"));
        assert!(html.contains("href=\"/download\""));
        assert!(html.contains("Not built yet"));
        assert!(html.contains("No Windows build"));
    }
}

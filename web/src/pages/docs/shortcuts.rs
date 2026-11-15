//! `/docs/shortcuts`: keyboard shortcuts per app. Every entry is bound in
//! the app's code (and matches its README where the README lists it):
//! `shared/ui/src/app.rs`, `project_bar.rs`, `text_input.rs`,
//! `editor/crates/core/src/commands.rs`, each app's `app.rs`, and
//! `term/apps/gui/src/input.rs`. Keys not bound in code are left out.

use maud::{Markup, html};

use super::{article, trail};
use crate::components::code;
use crate::layout::PageMeta;
use crate::products::{self, Product};

pub const PATH: &str = "/docs/shortcuts";
pub const TITLE: &str = "Keyboard shortcuts";

/// Key alternatives (each a combination or chord) and what they do.
type Shortcut = (&'static [&'static str], &'static str);

const SHARED: &[Shortcut] = &[
    (&["⌘Q"], "Quit"),
    (&["⎋"], "Close a menu, dialog or popup"),
    (&["↑", "↓"], "Move through menus and pickers"),
    (&["↩"], "Pick the selected item"),
    (
        &["⌘↩"],
        "Submit the focused field (what that does depends on the app)",
    ),
    (&["⌥⌘O"], "Open the project switcher, in apps with one"),
];

const EDITOR: &[Shortcut] = &[
    (&["⇧⌘P"], "Command palette"),
    (&["⌘P", "⌘T"], "Find a file"),
    (&["⌃G"], "Go to line"),
    (&["⌘F"], "Find"),
    (&["⌥⌘F"], "Find and replace"),
    (&["⌘G", "⇧⌘G"], "Next / previous match"),
    (&["⇧⌘F"], "Find in files"),
    (&["⌘S"], "Save"),
    (&["⌥⌘S"], "Save all"),
    (&["⌘W"], "Close tab"),
    (&["⇧⌘T"], "Reopen closed tab"),
    (&["⌘K →", "⌘K ↓"], "Split right / split down"),
    (&["⌘\\"], "Show or hide the file tree"),
    (&["⌘D"], "Select the next occurrence"),
    (&["⇧⌘L"], "Split selection into lines"),
    (&["⌘/"], "Toggle comment"),
    (&["⌃⌘↑", "⌃⌘↓"], "Move line up / down"),
    (&["F12", "⇧F12"], "Go to definition / find references"),
    (&["F2"], "Rename symbol"),
    (&["⌘."], "Code actions"),
    (&["⌃Space", "⌘I"], "Show completions"),
    (&["⌥⌘G"], "Git pane (FerrumGIT)"),
    (&["⌥⌘A"], "Agent pane (FerrumChat)"),
    (&["⌃`"], "Term pane (FerrumTerm)"),
    (&["⇧⌘I"], "CI page (GitHub Actions)"),
    (&["⌥⌘L"], "Logs page"),
    (&["⌘B", "⇧⌘B"], "Run the build task / pick a task"),
    (&["F5", "F9"], "Debugger: continue / toggle breakpoint"),
    (&["F10", "F11"], "Debugger: step over / step into"),
    (&["⇧⌘M"], "Markdown preview"),
    (&["⌘,"], "Settings"),
    (&["⌥⌘,"], "Open the config file (config.toml)"),
];

const API: &[Shortcut] = &[
    (
        &["⌘↩"],
        "Send the request (connect / disconnect a WebSocket)",
    ),
    (&["⌘S"], "Save the request"),
    (&["⌘F"], "Search the response body"),
    (&["⌘G", "⇧⌘G"], "Next / previous match"),
];

const GIT: &[Shortcut] = &[
    (&["⌘P", "⌘O"], "Quick open"),
    (&["⌘R"], "Reload"),
    (&["⌘[", "⌘]"], "Back / forward"),
    (&["⌘↩"], "Commit (in the commit message)"),
    (&["⌥⌘O"], "Project switcher"),
];

const DATA: &[Shortcut] = &[
    (
        &["⌘↩"],
        "Run the selection, or the statement under the caret",
    ),
    (&["⌘C"], "Copy the selected cells (as TSV)"),
    (&["⌘A"], "Select all cells"),
];

const TERM: &[Shortcut] = &[
    (&["⌘T"], "New tab"),
    (&["⌘N"], "New session"),
    (&["⌘D", "⇧⌘D"], "Split right / split down"),
    (&["⌘W"], "Close pane or tab"),
    (&["⌘]", "⌘["], "Next / previous pane"),
    (&["⇧⌘]", "⇧⌘["], "Next / previous tab"),
    (&["⌘1…9"], "Go to tab 1 to 9"),
    (&["⇧⌘S"], "Next session"),
    (&["⌘B"], "Show or hide the sidebar"),
    (&["⌘C", "⇧⌘C"], "Copy selection / copy screen"),
    (&["⌘V"], "Paste"),
    (&["⌘=", "⌘-", "⌘0"], "Font bigger / smaller / reset"),
    (&["⌘PgUp", "⌘PgDn"], "Scroll back / forward a page"),
    (&["⌥⌘O"], "Project switcher"),
];

const VIBE: &[Shortcut] = &[
    (&["⌥⌘1"], "Show or hide the chat column"),
    (&["⌥⌘2"], "Show or hide the git column"),
    (&["⌥⌘3"], "Show or hide the terminal column"),
    (&["⌥⌘O"], "Project switcher"),
    (&["↩", "⇧↩"], "Send / new line (in the chat)"),
    (&["⇧⇥"], "Switch between Agent, Ask and Plan (in the chat)"),
    (&["⌘."], "Stop the agent's turn"),
    (
        &["⌥⌘N", "⌥⌘J", "⌥⌘R"],
        "New thread / threads / changes (in the chat)",
    ),
    (
        &["⇧⌘↩", "⇧⌘⌫"],
        "Accept all / reject all changes (in the chat)",
    ),
];

const KIT: &[Shortcut] = &[
    (&["⌘K"], "Search the tools"),
    (&["↩"], "Open the first match (in the search box)"),
];

const DOCK: &[Shortcut] = &[
    (&["⌘R"], "Refresh (or retry while the engine is offline)"),
    (&["⌘F"], "Filter"),
    (&["⎋"], "Close the dialog or detail panel"),
    (&["⌥⌘O"], "Project switcher"),
];

const PORTS: &[Shortcut] = &[
    (&["⌘R"], "Refresh"),
    (&["⌘F"], "Search"),
    (&["⇧⌘P"], "Pause or resume auto-refresh"),
    (&["⌥⌘O"], "Project switcher"),
];

const SYSTEM: &[Shortcut] = &[
    (&["⌘="], "Zoom in"),
    (&["⌘-"], "Zoom out"),
    (&["⌘0"], "Fit the diagram"),
    (&["⌫"], "Delete the selected component"),
    (
        &["Esc"],
        "Close a dialog, stop connecting, clear the selection",
    ),
    (&["⌘,"], "Settings"),
];

const ENV: &[Shortcut] = &[
    (&["⌘L"], "Lock the vault"),
    (&["⌘N"], "New variable"),
    (&["⌘,"], "Settings"),
    (&["⌥⌘O"], "Project switcher"),
];

/// Every app with its shortcuts, in site order.
pub const APPS: [(Product, &[Shortcut]); 11] = [
    (products::FERRUMIDE, EDITOR),
    (products::FERRUMENGINEER, VIBE),
    (products::FERRUM_SYSTEM, SYSTEM),
    (products::FERRUMAPI, API),
    (products::FERRUMGIT, GIT),
    (products::FERRUMDATA, DATA),
    (products::FERRUMTERM, TERM),
    (products::FERRUMKIT, KIT),
    (products::FERRUMCONTAINERS, DOCK),
    (products::FERRUMPORTS, PORTS),
    (products::FERRUMVARIABLES, ENV),
];

fn table(caption: &str, rows: &[Shortcut]) -> Markup {
    html! {
        div class="table-wrap" {
            table class="data-table shortcut-table" {
                caption class="visually-hidden" { (caption) }
                thead { tr { th scope="col" { "Keys" } th scope="col" { "Action" } } }
                tbody {
                    @for (keys, action) in rows {
                        tr {
                            td {
                                @for (i, k) in keys.iter().enumerate() {
                                    @if i > 0 { span class="keys-or" { " or " } }
                                    (code::keys(k))
                                }
                            }
                            td { (action) }
                        }
                    }
                }
            }
        }
    }
}

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Keyboard shortcuts for every Ferrum app"),
        "macOS keyboard shortcuts for FerrumIDE, FerrumEngineer, FerrumAPI, FerrumGIT, \
         FerrumData, FerrumTerm, FerrumKit, FerrumContainers, FerrumPorts and FerrumVariables.",
        PATH,
        "page-docs",
    )
    .trail(trail("Shortcuts", PATH));
    article(
        site_url,
        &meta,
        TITLE,
        html! {
            p {
                "The keys each app binds on macOS. On Windows and Linux, " kbd { "Ctrl" }
                " takes the place of " kbd { "⌘" } "."
            }
        },
        html! {
            nav class="jump-list" aria-label="Apps on this page" {
                ul {
                    li { a href="#shared" { "Every app" } }
                    @for (p, _) in &APPS {
                        li style=(p.accent_style()) { a class="app-dot" href={ "#" (p.slug) } { (p.name) } }
                    }
                    li { a href="#launcher" { "Ferrum Launcher" } }
                }
            }

            h2 id="shared" { "In every app" }
            p {
                "Text fields also take the usual macOS keys: select all, copy, cut, paste, and "
                "moving by word or line with " kbd { "⌥" } " or " kbd { "⌘" } " and the arrows."
            }
            (table("Shortcuts shared by every app", SHARED))

            @for (p, rows) in &APPS {
                section class="shortcut-app" style=(p.accent_style()) aria-labelledby=(p.slug) {
                    h2 id=(p.slug) {
                        (p.name)
                    }
                    (table(&format!("{} shortcuts", p.name), rows))
                    @if p.slug == products::FERRUMIDE.slug {
                        p class="muted" {
                            "These are the defaults. Rebind any command in "
                            code { "keymap.toml" } "; " (code::keys("⇧⌘P")) " lists every \
                             command with its current keys."
                        }
                    }
                    p { a href=(p.path()) { "More about " (p.name) } }
                }
            }

            h2 id="launcher" { "Ferrum Launcher" }
            p {
                "The launcher has no shortcuts of its own beyond " (code::keys("⌘Q")) ". On each \
                 app's page you can record a global " strong { "Shortcut" } ": ⌘, ⌃ or ⌥ plus a \
                 letter, digit, F-key or space. It opens or brings forward that app from \
                 anywhere while the launcher runs. Press " (code::keys("⎋")) " to cancel \
                 recording."
            }
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn has_a_table_per_app_and_spoken_keys() {
        let html = render("https://f.test").into_string();
        assert_eq!(html.matches("<h1").count(), 1);
        for (p, _) in &APPS {
            assert!(
                html.contains(&format!(r#"<h2 id="{}">"#, p.slug)),
                "{}",
                p.slug
            );
        }
        assert!(html.contains("Shift Command P"));
        assert!(
            EDITOR
                .iter()
                .any(|(keys, what)| keys == &["⌘,"] && *what == "Settings")
        );
        assert!(
            EDITOR
                .iter()
                .any(|(keys, what)| keys == &["⌥⌘,"] && what.starts_with("Open the config file"))
        );
        assert_eq!(APPS.len(), products::apps().count());
    }
}

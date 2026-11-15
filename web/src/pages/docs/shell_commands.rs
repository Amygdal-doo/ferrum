//! `/docs/shell-commands`: `fei .` and friends. The defaults and behaviour
//! come from `launcher/crates/core/src/shell.rs` and launcher/README.md.

use maud::{Markup, html};

use super::{article, trail};
use crate::components::{
    callout::{self, Kind},
    card,
};
use crate::layout::PageMeta;
use crate::products::{self, Product};

pub const PATH: &str = "/docs/shell-commands";
pub const TITLE: &str = "Shell commands";

/// Suggested command per app (`shell::default_command`), with what it does.
pub const COMMANDS: [(&str, Product, &str); 12] = [
    (
        "fei",
        products::FERRUMIDE,
        "Opens FerrumIDE; `fei .` opens the current folder",
    ),
    (
        "fen",
        products::FERRUMENGINEER,
        "Opens FerrumEngineer; `fen .` opens it on the current folder",
    ),
    (
        "fes",
        products::FERRUM_SYSTEM,
        "Opens FerrumSystem; `fes .` opens the current folder's architecture",
    ),
    ("fea", products::FERRUMAPI, "Opens FerrumAPI"),
    (
        "feg",
        products::FERRUMGIT,
        "Opens FerrumGIT; `feg .` opens the repository in this folder",
    ),
    ("fed", products::FERRUMDATA, "Opens FerrumData"),
    (
        "fet",
        products::FERRUMTERM,
        "Opens FerrumTerm with a session in the current folder (or the folder given)",
    ),
    ("fek", products::FERRUMKIT, "Opens FerrumKit"),
    (
        "fec",
        products::FERRUMCONTAINERS,
        "Opens FerrumContainers; fec start, stop, up, down and any docker command",
    ),
    ("fep", products::FERRUMPORTS, "Opens FerrumPorts"),
    ("fev", products::FERRUMVARIABLES, "Opens FerrumVariables"),
    ("fel", products::FERRUM_LAUNCHER, "Opens FerrumLauncher"),
];

/// Renders "`x` text" descriptions: backticked spans become `<code>`.
fn with_code(text: &str) -> Markup {
    html! {
        @for (i, part) in text.split('`').enumerate() {
            @if i % 2 == 1 { code { (part) } } @else { (part) }
        }
    }
}

pub fn render(site_url: &str) -> Markup {
    let meta = PageMeta::new(
        Some("Shell commands: fei, feg, fet and more"),
        "Open Ferrum apps from a terminal: fei for FerrumIDE, feg for FerrumGIT, fet for \
         FerrumTerm, fen for FerrumEngineer, fes for FerrumSystem, and fea, fed, fek, fec, fep, \
         fev and fel for the rest.",
        PATH,
        "page-docs",
    )
    .trail(trail(TITLE, PATH));
    article(
        site_url,
        &meta,
        TITLE,
        html! {
            p {
                "Open any Ferrum app from a terminal, the way " code { "zed ." } " or "
                code { "code ." } " work. Each command is off until you turn it on in \
                 FerrumLauncher, and you can rename it."
            }
        },
        html! {
            h2 id="turn-on" { "Turn a command on" }
            ol class="prose-steps" {
                li { "Open FerrumLauncher and click the app." }
                li {
                    "Turn on " strong { "Shell command" } ". The name field suggests the default \
                     below; type another name and press Return to rename it."
                }
                li { "Open a new terminal window and run it, for example " code { "fei ." } "." }
            }

            h2 id="defaults" { "The default names" }
            p { "Each default is \"fe\" plus a letter for the app." }
            div class="table-wrap" {
                table class="data-table" {
                    caption class="visually-hidden" { "Default shell command for each app" }
                    thead { tr { th scope="col" { "Command" } th scope="col" { "App" } th scope="col" { "What it does" } } }
                    tbody {
                        @for (cmd, p, what) in &COMMANDS {
                            tr style=(p.accent_style()) {
                                th scope="row" { code { (cmd) } }
                                td { a class="app-dot" href=(p.path()) { (p.name) } }
                                td { (with_code(what)) }
                            }
                        }
                    }
                }
            }
            (callout::render(Kind::Note, Some("Renamed apps, new commands"), html! {
                p {
                    "Ferrum Editor is now FerrumIDE, FerrumVibe FerrumEngineer and FerrumEnv \
                     FerrumVariables, and their commands follow the new letters: "
                    code { "fee" } " became " code { "fei" } ", " code { "fen" } " (which opened \
                     FerrumEnv) now opens FerrumEngineer, and " code { "fev" } " (which opened \
                     FerrumVibe) now opens FerrumVariables. FerrumLauncher moves commands you \
                     already turned on by itself; names you chose yourself are left alone."
                }
            }))
            p {
                "Arguments are passed through to the app. If the app isn't installed, the \
                 command says so and points you to the launcher."
            }

            h2 id="how" { "How they work" }
            p {
                "Each command is a small script in " code { "~/.ferrum/bin" } ", next to the \
                 apps' own command-line links. It starts the active version of the app in the \
                 background, so your terminal stays free."
            }
            p {
                "Turning on the first command also adds " code { "~/.ferrum/bin" } " to your \
                 PATH, with one marked line in your shell's startup files:"
            }
            dl class="spec-sheet" {
                div class="spec-sheet__row" { dt { "zsh" } dd { code { "~/.zprofile" } " and " code { "~/.zshrc" } } }
                div class="spec-sheet__row" { dt { "bash" } dd { code { "~/.bash_profile" } " and " code { "~/.bashrc" } } }
                div class="spec-sheet__row" { dt { "fish" } dd { code { "~/.config/fish/conf.d/ferrum.fish" } } }
            }
            p {
                "So the commands work in Terminal, in editor and IDE terminals, in tmux, and in \
                 FerrumTerm and FerrumIDE's Term pane."
            }
            (callout::render(Kind::Note, Some("Your own files are safe"), html! {
                p {
                    "Command names use lowercase letters, digits, " code { "-" } " and "
                    code { "_" } ", starting with a letter. The launcher only replaces or \
                     removes scripts it wrote itself, and refuses a name that is already taken \
                     by a file it didn't make."
                }
            }))

            h2 id="more" { "More ways to open apps" }
            p {
                "Each app's page in the launcher also has " strong { "Hide from Dock" } " (the \
                 app runs without a Dock icon) and a global " strong { "Shortcut" } " (⌘, ⌃ or ⌥ \
                 plus a letter, digit, F-key or space) that brings the app forward while the \
                 launcher runs. See " a href=(super::shortcuts::PATH) { "keyboard shortcuts" } "."
            }
            (card::app_strip(COMMANDS.iter().map(|(_, p, _)| p), false))
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lists_every_default_command() {
        let html = render("https://f.test").into_string();
        for (cmd, p, _) in &COMMANDS {
            assert!(html.contains(&format!("<code>{cmd}</code>")), "{cmd}");
            assert!(html.contains(&format!(r#"href="{}""#, p.path())));
        }
        assert!(html.contains("<code>fei .</code>"));
        assert!(html.contains("<code>fen</code>"));
        assert!(html.contains("<code>fev</code>"));
        assert_eq!(COMMANDS.len(), products::ALL.len());
    }
}

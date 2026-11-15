//! "Ferrum Editor, FerrumVibe and FerrumEnv get new names" (2026-11-15).
//! Claims follow `catalog.json` (`former_names`), `brand/generate_svg.py`,
//! `launcher/crates/core/src/shell.rs` and `store.rs` (the shell-command
//! move) and the IDE's key bindings (⌘, and ⌥⌘,).

use maud::{Markup, html};

use super::product_row;
use crate::products::{FERRUMENGINEER, FERRUMIDE, FERRUMVARIABLES, Product};
use crate::updates::Post;

pub const POST: Post = Post {
    slug: "ferrumide-ferrumengineer-and-ferrumvariables",
    title: "Ferrum Editor, FerrumVibe and FerrumEnv get new names",
    date: "2026-11-15",
    byline: "Ferrum Team",
    summary: "Ferrum Editor is now FerrumIDE, FerrumVibe is FerrumEngineer and FerrumEnv is \
              FerrumVariables, with new logos and shell commands. Installed apps update in \
              place and keep their settings.",
    body,
};

/// The renamed apps: (product, former name, new logo and command).
const RENAMED: [(Product, &str, &str, &str); 3] = [
    (FERRUMIDE, "Ferrum Editor", "FeI", "fei"),
    (FERRUMENGINEER, "FerrumVibe", "FeN", "fen"),
    (FERRUMVARIABLES, "FerrumEnv", "FeV", "fev"),
];

fn body() -> Markup {
    html! {
        p class="post__intro" {
            "Three apps get names that say what they are. Nothing else about them changes."
        }
        ul class="post-apps" {
            @for (p, former, logo, command) in RENAMED {
                (product_row(&p, &format!("Formerly {former}. Logo {logo}, shell command {command}.")))
            }
        }

        h2 id="names" { "The new names" }
        ul {
            li {
                strong { "FerrumIDE" } " is the suite's code editor, with Git, Agent and Term \
                 panes, the Logs and CI pages and the debugger built in. Its logo is FeI and \
                 its command " code { "fei" } "."
            }
            li {
                strong { "FerrumEngineer" } " (FerrumVibe until today, and FerrumAgentic before \
                 that) keeps an agent live in your project with its threads, FerrumGIT and \
                 FerrumTerm. Its logo is FeN and its command " code { "fen" } "."
            }
            li {
                strong { "FerrumVariables" } " is the password-locked vault for your projects' \
                 environment variables. Its logo is FeV and its command " code { "fev" } "."
            }
        }

        h2 id="existing" { "If you already use them" }
        p {
            "The apps update in place. Their ids, programs and data folders stay the same, so \
             your projects, threads, vault and settings are where they were, in "
            code { "~/.ferrum" } ". Ferrum Launcher replaces the copy in "
            code { "~/Applications/Ferrum" } " (or the Start Menu shortcut on Windows) with \
             one under the new name."
        }
        p {
            "Shell commands you turned on move by themselves: " code { "fee" } " becomes "
            code { "fei" } ", the " code { "fen" } " that opened FerrumEnv now opens \
             FerrumEngineer, and FerrumVariables gets " code { "fev" } ". A command you named \
             yourself is left alone. See "
            a href=(crate::pages::docs::shell_commands::PATH) { "shell commands" } "."
        }
        p {
            "The old web addresses redirect to the new pages, so links and bookmarks keep \
             working."
        }

        h2 id="settings" { "⌘, opens Settings in FerrumIDE" }
        p {
            "Like every other Ferrum app, FerrumIDE now opens its Settings window with "
            kbd { "⌘," } ". The config file, " code { "~/.ferrum/editor/config.toml" } ", moves \
             to " kbd { "⌥⌘," } ". See the "
            a href=(crate::pages::docs::shortcuts::PATH) { "keyboard shortcuts" } "."
        }
        p {
            "Every release is also on the " a href=(crate::pages::changelog::PATH) { "changelog" } "."
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_each_app_its_former_name_and_command() {
        let html = body().into_string();
        for (p, former, logo, command) in RENAMED {
            assert!(
                html.contains(&format!(r#"href="{}""#, p.path())),
                "{}",
                p.name
            );
            assert!(html.contains(former), "{former}");
            assert!(html.contains(logo), "{logo}");
            assert!(
                html.contains(&format!("<code>{command}</code>")),
                "{command}"
            );
        }
        assert!(html.contains("<code>fee</code>"));
        assert!(html.contains("<kbd>⌥⌘,</kbd>"));
    }
}

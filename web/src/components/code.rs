//! Code, commands and keys.
//!
//! - [`block`]: a `<pre><code>` block. Class `.code-block`.
//! - [`command`]: one shell command with a copy button. Class `.install-cmd`.
//! - [`keys`]: a key combination such as `⌥⌘O`, drawn as key caps with a
//!   spelled-out text alternative ("Option Command O"). Class `.keys`.

use maud::{Markup, html};

pub fn block(text: &str) -> Markup {
    html! { pre class="code-block" { code { (text) } } }
}

/// A one-line command with a copy button (`js/site.js` handles `data-copy`).
pub fn command(cmd: &str) -> Markup {
    html! {
        div class="install-cmd" {
            code { (cmd) }
            button class="install-cmd__copy" type="button" data-copy=(cmd)
                aria-label="Copy command" { "Copy" }
        }
    }
}

/// Spoken name of a modifier or special key glyph.
fn key_name(key: &str) -> &str {
    match key {
        "⌘" => "Command",
        "⌥" => "Option",
        "⇧" => "Shift",
        "⌃" => "Control",
        "↩" => "Return",
        "⎋" => "Escape",
        "⇥" => "Tab",
        "⌫" => "Delete",
        "↑" => "Up Arrow",
        "↓" => "Down Arrow",
        "←" => "Left Arrow",
        "→" => "Right Arrow",
        "\\" => "Backslash",
        "`" => "Backtick",
        other => other,
    }
}

/// Splits `⇧⌘P` into `["⇧", "⌘", "P"]`: each modifier glyph is a key, the
/// rest (`P`, `F12`, `Space`, `PgUp`) is the final key.
fn split_keys(combo: &str) -> Vec<String> {
    const MODIFIERS: [char; 4] = ['⌘', '⌥', '⇧', '⌃'];
    let mut keys = Vec::new();
    let mut rest = combo.trim();
    while let Some(c) = rest.chars().next().filter(|c| MODIFIERS.contains(c)) {
        keys.push(c.to_string());
        rest = &rest[c.len_utf8()..];
    }
    if !rest.is_empty() {
        keys.push(rest.to_owned());
    }
    keys
}

/// One combination, e.g. `keys("⌥⌘O")`, or a chord whose steps are
/// separated by spaces (`keys("⌘K →")`: ⌘K, then →).
pub fn keys(combo: &str) -> Markup {
    let steps: Vec<Vec<String>> = combo.split_whitespace().map(split_keys).collect();
    let spoken = steps
        .iter()
        .map(|step| {
            step.iter()
                .map(|k| key_name(k))
                .collect::<Vec<_>>()
                .join(" ")
        })
        .collect::<Vec<_>>()
        .join(", then ");
    html! {
        kbd class="keys" {
            span aria-hidden="true" {
                @for (i, step) in steps.iter().enumerate() {
                    @if i > 0 { span class="keys__then" { " " } }
                    @for k in step { kbd { (k) } }
                }
            }
            span class="visually-hidden" { (spoken) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_modifiers_from_the_key() {
        assert_eq!(split_keys("⇧⌘P"), ["⇧", "⌘", "P"]);
        assert_eq!(split_keys("F12"), ["F12"]);
        assert_eq!(split_keys("⌘PgUp"), ["⌘", "PgUp"]);
        assert_eq!(split_keys("⌃`"), ["⌃", "`"]);
    }

    #[test]
    fn keys_have_a_spoken_alternative() {
        let html = keys("⌥⌘O").into_string();
        assert_eq!(html.matches("<kbd>").count(), 3);
        assert!(html.contains("Option Command O"));
        let chord = keys("⌘K →").into_string();
        assert_eq!(chord.matches("<kbd>").count(), 3);
        assert!(chord.contains("Command K, then Right Arrow"));
    }

    #[test]
    fn command_has_a_copy_button() {
        let html = command("echo hi").into_string();
        assert!(html.contains(r#"data-copy="echo hi""#));
    }
}

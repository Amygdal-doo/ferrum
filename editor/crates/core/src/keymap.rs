//! Key bindings: defaults from the command catalogue plus the user's
//! `~/.ferrum/editor/keymap.toml`:
//!
//! ```toml
//! [bindings]
//! "cmd-shift-d" = "editor:duplicate-lines"   # bind (replaces any default on these keys)
//! "cmd-d" = ""                               # unbind
//! "cmd-k cmd-c" = "editor:toggle-comment"    # multi-key sequence
//! ```
//!
//! Keys are written like `ctrl-alt-shift-cmd-x` (any modifier order;
//! aliases: `command`/`super`/`meta`/`win` → `cmd`, `option` → `alt`,
//! `control` → `ctrl`). On Linux and Windows `cmd` means `ctrl`.

use std::collections::{BTreeMap, HashMap};

use crate::commands::{CommandInfo, Scope};

const MODIFIER_ORDER: [&str; 5] = ["ctrl", "alt", "shift", "cmd", "fn"];
/// Defaults that only make sense on macOS (Emacs-style keys, and Cmd+arrow
/// motions whose Ctrl translation means "word" elsewhere).
const MAC_ONLY: [&str; 16] = [
    "ctrl-a",
    "ctrl-e",
    "ctrl-b",
    "ctrl-f",
    "ctrl-n",
    "ctrl-p",
    "cmd-left",
    "cmd-right",
    "shift-cmd-left",
    "shift-cmd-right",
    "cmd-up",
    "cmd-down",
    "shift-cmd-up",
    "shift-cmd-down",
    "cmd-backspace",
    "cmd-delete",
];
/// Word motions use Alt on macOS and Ctrl elsewhere.
const NON_MAC_RENAMES: [(&str, &str); 6] = [
    ("alt-left", "ctrl-left"),
    ("alt-right", "ctrl-right"),
    ("alt-shift-left", "ctrl-shift-left"),
    ("alt-shift-right", "ctrl-shift-right"),
    ("alt-backspace", "ctrl-backspace"),
    ("alt-delete", "ctrl-delete"),
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Binding {
    /// Normalised keystroke sequence, e.g. `cmd-k cmd-u`.
    pub keys: String,
    pub command: String,
    pub scope: Scope,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Keymap {
    pub bindings: Vec<Binding>,
}

/// Normalises one keystroke (`Shift+Cmd+P` → `shift-cmd-p`).
pub fn normalize_keystroke(keystroke: &str) -> Result<String, String> {
    let lower = keystroke.trim().to_lowercase().replace('+', "-");
    let lower = if keystroke.trim().ends_with('+') || lower.ends_with("--") || lower == "-" {
        // `cmd--` / `cmd-+`: the key itself is `-` or `+`.
        let key = if keystroke.trim().ends_with('+') {
            "+"
        } else {
            "-"
        };
        let rest = lower.trim_end_matches('-');
        return assemble(
            if rest.is_empty() {
                Vec::new()
            } else {
                rest.split('-').collect()
            },
            key,
            keystroke,
        );
    } else {
        lower
    };
    let mut parts: Vec<&str> = lower.split('-').collect();
    let key = parts.pop().unwrap_or_default();
    assemble(parts, key, keystroke)
}

fn assemble(modifiers: Vec<&str>, key: &str, original: &str) -> Result<String, String> {
    if key.is_empty() {
        return Err(format!("`{original}`: missing key"));
    }
    let mut found = [false; 5];
    for modifier in modifiers {
        let canonical = match modifier {
            "cmd" | "command" | "super" | "meta" | "win" => "cmd",
            "alt" | "option" | "opt" => "alt",
            "ctrl" | "control" => "ctrl",
            "shift" => "shift",
            "fn" => "fn",
            other => return Err(format!("`{original}`: unknown modifier `{other}`")),
        };
        if let Some(i) = MODIFIER_ORDER.iter().position(|m| *m == canonical) {
            found[i] = true;
        }
    }
    let mut out: Vec<&str> = MODIFIER_ORDER
        .iter()
        .zip(found)
        .filter(|(_, on)| *on)
        .map(|(m, _)| *m)
        .collect();
    out.push(key);
    Ok(out.join("-"))
}

/// Normalises a space-separated keystroke sequence.
pub fn normalize_keys(keys: &str) -> Result<String, String> {
    let strokes: Result<Vec<String>, String> =
        keys.split_whitespace().map(normalize_keystroke).collect();
    let strokes = strokes?;
    if strokes.is_empty() {
        return Err("empty key binding".into());
    }
    Ok(strokes.join(" "))
}

/// `cmd` → `ctrl` for Linux / Windows.
fn for_platform(keys: &str, mac: bool) -> Option<String> {
    if mac {
        return Some(keys.to_string());
    }
    if MAC_ONLY.contains(&keys) {
        return None;
    }
    if let Some((_, renamed)) = NON_MAC_RENAMES.iter().find(|(from, _)| *from == keys) {
        return Some((*renamed).to_string());
    }
    let strokes: Vec<String> = keys
        .split(' ')
        .map(|stroke| {
            let parts: Vec<&str> = stroke.split('-').collect();
            let (key, mods) = parts.split_last().unwrap_or((&"", &[]));
            let mods: Vec<&str> = mods
                .iter()
                .map(|m| if *m == "cmd" { "ctrl" } else { m })
                .collect();
            assemble(mods, key, stroke).unwrap_or_else(|_| stroke.to_string())
        })
        .collect();
    Some(strokes.join(" "))
}

impl Keymap {
    /// Default bindings for every command. On non-mac platforms a key that
    /// two commands would share after translation goes to the first.
    pub fn defaults(commands: &[CommandInfo], mac: bool) -> Self {
        let mut bindings: Vec<Binding> = Vec::new();
        for command in commands {
            for keys in &command.default_keys {
                let Ok(keys) = normalize_keys(keys) else {
                    continue;
                };
                let Some(keys) = for_platform(&keys, mac) else {
                    continue;
                };
                if bindings
                    .iter()
                    .any(|b| b.keys == keys && b.scope == command.scope)
                {
                    continue;
                }
                bindings.push(Binding {
                    keys,
                    command: command.name.clone(),
                    scope: command.scope,
                });
            }
        }
        Self { bindings }
    }

    /// Applies the user's keymap on top of `self`. Returns warnings for
    /// unknown commands and malformed keys (those entries are skipped).
    pub fn apply_user(
        &mut self,
        user: &UserKeymap,
        commands: &[CommandInfo],
        mac: bool,
    ) -> Vec<String> {
        let scopes: HashMap<&str, Scope> = commands
            .iter()
            .map(|c| (c.name.as_str(), c.scope))
            .collect();
        let mut warnings = Vec::new();
        for (raw_keys, command) in &user.bindings {
            let keys = match normalize_keys(raw_keys) {
                Ok(keys) => for_platform(&keys, mac).unwrap_or(keys),
                Err(error) => {
                    warnings.push(error);
                    continue;
                }
            };
            let command = command.trim();
            if !command.is_empty() && !scopes.contains_key(command) {
                warnings.push(format!("`{raw_keys}`: unknown command `{command}`"));
                continue;
            }
            self.bindings.retain(|b| b.keys != keys);
            if let Some(scope) = scopes.get(command) {
                self.bindings.push(Binding {
                    keys,
                    command: command.to_string(),
                    scope: *scope,
                });
            }
        }
        warnings
    }

    /// Key sequences bound to `command`, in binding order.
    pub fn keys_for(&self, command: &str) -> Vec<&str> {
        self.bindings
            .iter()
            .filter(|b| b.command == command)
            .map(|b| b.keys.as_str())
            .collect()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UserKeymap {
    /// (keys, command); an empty command unbinds the keys.
    pub bindings: Vec<(String, String)>,
}

impl UserKeymap {
    pub fn parse(source: &str) -> Result<Self, String> {
        #[derive(serde::Deserialize, Default)]
        struct File {
            #[serde(default)]
            bindings: BTreeMap<String, String>,
        }
        let file: File = toml::from_str(source).map_err(|e| e.message().to_string())?;
        Ok(Self {
            bindings: file.bindings.into_iter().collect(),
        })
    }
}

/// Human-readable keys: `⇧⌘P` on macOS, `Ctrl+Shift+P` elsewhere.
pub fn display_keys(keys: &str, mac: bool) -> String {
    keys.split(' ')
        .map(|stroke| {
            let parts: Vec<&str> = stroke.split('-').collect();
            let (key, mods) = match parts.split_last() {
                Some((key, mods)) if !key.is_empty() => (*key, mods.to_vec()),
                _ => ("-", parts[..parts.len().saturating_sub(2)].to_vec()),
            };
            let key = display_key(key, mac);
            if mac {
                let order = [
                    ("ctrl", "⌃"),
                    ("alt", "⌥"),
                    ("shift", "⇧"),
                    ("cmd", "⌘"),
                    ("fn", "fn"),
                ];
                let mut out: String = order
                    .iter()
                    .filter(|(m, _)| mods.contains(m))
                    .map(|(_, s)| *s)
                    .collect();
                out.push_str(&key);
                out
            } else {
                let mut out: Vec<String> = mods
                    .iter()
                    .map(|m| match *m {
                        "ctrl" => "Ctrl".to_string(),
                        "alt" => "Alt".to_string(),
                        "shift" => "Shift".to_string(),
                        "cmd" => "Super".to_string(),
                        other => other.to_string(),
                    })
                    .collect();
                out.push(key);
                out.join("+")
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn display_key(key: &str, mac: bool) -> String {
    let symbol = match key {
        "up" => "↑",
        "down" => "↓",
        "left" => "←",
        "right" => "→",
        "enter" if mac => "↩",
        "backspace" if mac => "⌫",
        "delete" if mac => "⌦",
        "escape" if mac => "⎋",
        "tab" if mac => "⇥",
        "space" => "Space",
        "pageup" => "PgUp",
        "pagedown" => "PgDn",
        other => return other.to_uppercase(),
    };
    symbol.to_string()
}

/// The default `keymap.toml`: documentation of every command and its
/// current default keys, with an empty `[bindings]` table to fill in.
pub fn default_keymap_document(commands: &[CommandInfo], mac: bool) -> String {
    let defaults = Keymap::defaults(commands, mac);
    let mut out = String::from(
        "# Ferrum Editor key bindings. Changes apply as soon as you save.\n\
         #\n\
         # Bind a key sequence to any command name below; an empty string unbinds:\n\
         #   \"cmd-shift-d\" = \"editor:duplicate-lines\"\n\
         #   \"cmd-k cmd-c\" = \"editor:toggle-comment\"\n\
         #   \"cmd-d\" = \"\"\n\
         # Modifiers: ctrl alt shift cmd (cmd = Ctrl on Linux/Windows).\n\
         #\n\
         # Commands and their default keys:\n",
    );
    for command in commands {
        let keys = defaults.keys_for(&command.name);
        let keys = if keys.is_empty() {
            "—".to_string()
        } else {
            keys.join(", ")
        };
        out.push_str(&format!("#   {:<44} {keys}\n", command.name));
    }
    out.push_str("\n[bindings]\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::builtin_commands;

    #[test]
    fn normalises_keystrokes() {
        assert_eq!(normalize_keystroke("Shift+Cmd+P"), Ok("shift-cmd-p".into()));
        assert_eq!(
            normalize_keystroke("cmd-alt-ctrl-x"),
            Ok("ctrl-alt-cmd-x".into())
        );
        assert_eq!(
            normalize_keystroke("option-command-up"),
            Ok("alt-cmd-up".into())
        );
        assert_eq!(normalize_keystroke("cmd--"), Ok("cmd--".into()));
        assert_eq!(normalize_keystroke("cmd-+"), Ok("cmd-+".into()));
        assert!(normalize_keystroke("hyper-x").is_err());
        assert_eq!(normalize_keys("cmd-k  CMD-U"), Ok("cmd-k cmd-u".into()));
        assert!(normalize_keys("  ").is_err());
    }

    #[test]
    fn defaults_cover_core_shortcuts() {
        let map = Keymap::defaults(&builtin_commands(), true);
        assert_eq!(map.keys_for("command-palette:toggle"), vec!["shift-cmd-p"]);
        assert_eq!(map.keys_for("editor:select-next"), vec!["cmd-d"]);
        assert_eq!(map.keys_for("pane:split-right"), vec!["cmd-k right"]);
    }

    #[test]
    fn non_mac_translates_cmd_and_drops_emacs_keys() {
        let map = Keymap::defaults(&builtin_commands(), false);
        assert_eq!(
            map.keys_for("fuzzy-finder:toggle-file-finder"),
            vec!["ctrl-p", "ctrl-t"]
        );
        assert_eq!(map.keys_for("editor:move-to-line-start"), vec!["home"]);
        assert_eq!(map.keys_for("editor:select-all"), vec!["ctrl-a"]);
    }

    #[test]
    fn user_keymap_rebinds_and_unbinds() {
        let commands = builtin_commands();
        let mut map = Keymap::defaults(&commands, true);
        let user = UserKeymap::parse(
            r#"
            [bindings]
            "cmd-d" = "editor:duplicate-lines"
            "cmd-shift-p" = ""
            "cmd-k cmd-c" = "editor:toggle-comment"
            "cmd-x" = "editor:no-such-thing"
            "hyper-q" = "editor:undo"
            "#,
        )
        .expect("valid toml");
        let warnings = map.apply_user(&user, &commands, true);
        assert_eq!(warnings.len(), 2);
        assert!(map.keys_for("editor:duplicate-lines").contains(&"cmd-d"));
        assert!(!map.keys_for("editor:select-next").contains(&"cmd-d"));
        assert!(map.keys_for("command-palette:toggle").is_empty());
        assert_eq!(
            map.keys_for("editor:toggle-comment"),
            vec!["cmd-/", "cmd-k cmd-c"]
        );
        assert_eq!(map.keys_for("editor:cut"), vec!["cmd-x"]);
    }

    #[test]
    fn malformed_user_keymap_is_an_error() {
        assert!(UserKeymap::parse("[bindings]\n\"x\" = 3").is_err());
        assert_eq!(UserKeymap::parse("").map(|k| k.bindings.len()), Ok(0));
    }

    #[test]
    fn displays_keys() {
        assert_eq!(display_keys("shift-cmd-p", true), "⇧⌘P");
        assert_eq!(display_keys("cmd-k cmd-u", true), "⌘K ⌘U");
        assert_eq!(display_keys("ctrl-shift-up", false), "Ctrl+Shift+↑");
        assert_eq!(display_keys("cmd--", true), "⌘-");
    }

    #[test]
    fn default_document_lists_every_command_and_parses() {
        let commands = builtin_commands();
        let doc = default_keymap_document(&commands, true);
        for command in &commands {
            assert!(doc.contains(&command.name));
        }
        assert_eq!(UserKeymap::parse(&doc).map(|k| k.bindings.len()), Ok(0));
    }
}

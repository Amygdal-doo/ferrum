//! GPUI keystrokes → FerrumTerm inputs or app shortcuts (Cmd+…).
//!
//! Plain printable characters are not handled here: they reach the view's
//! input handler (so IME composition works) and are sent as text.

use ferrum_ui::gpui::Keystroke;
use ferrumterm_protocol::Input;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shortcut {
    NewTab,
    SplitRight,
    SplitDown,
    CloseBlock,
    NextBlock,
    PrevBlock,
    NextTab,
    PrevTab,
    Tab(usize),
    Paste,
    CopyScreen,
    FontBigger,
    FontSmaller,
    FontReset,
    NextSession,
    NewSession,
    ScrollPageUp,
    ScrollPageDown,
    ToggleSidebar,
    Copy,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    Send(Input),
    Shortcut(Shortcut),
}

fn named(key: &str) -> Option<&'static str> {
    Some(match key {
        "enter" => "Enter",
        "tab" => "Tab",
        "escape" => "Escape",
        "backspace" => "BSpace",
        "space" => "Space",
        "up" => "Up",
        "down" => "Down",
        "left" => "Left",
        "right" => "Right",
        "home" => "Home",
        "end" => "End",
        "pageup" => "PageUp",
        "pagedown" => "PageDown",
        "insert" => "Insert",
        "delete" => "Delete",
        "f1" => "F1",
        "f2" => "F2",
        "f3" => "F3",
        "f4" => "F4",
        "f5" => "F5",
        "f6" => "F6",
        "f7" => "F7",
        "f8" => "F8",
        "f9" => "F9",
        "f10" => "F10",
        "f11" => "F11",
        "f12" => "F12",
        _ => return None,
    })
}

fn shortcut(key: &str, shift: bool) -> Option<Shortcut> {
    Some(match (key, shift) {
        ("t", false) => Shortcut::NewTab,
        ("n", false) => Shortcut::NewSession,
        ("d", false) => Shortcut::SplitRight,
        ("d", true) => Shortcut::SplitDown,
        ("w", _) => Shortcut::CloseBlock,
        ("]", false) => Shortcut::NextBlock,
        ("[", false) => Shortcut::PrevBlock,
        ("]", true) | ("}", _) => Shortcut::NextTab,
        ("[", true) | ("{", _) => Shortcut::PrevTab,
        ("v", _) => Shortcut::Paste,
        ("c", true) => Shortcut::CopyScreen,
        ("c", false) => Shortcut::Copy,
        ("=" | "+", _) => Shortcut::FontBigger,
        ("-", _) => Shortcut::FontSmaller,
        ("0", false) => Shortcut::FontReset,
        ("s", true) => Shortcut::NextSession,
        ("b", false) => Shortcut::ToggleSidebar,
        ("pageup", _) => Shortcut::ScrollPageUp,
        ("pagedown", _) => Shortcut::ScrollPageDown,
        (digit, false) if digit.len() == 1 && digit != "0" => {
            let byte = digit.as_bytes()[0];
            if !byte.is_ascii_digit() {
                return None;
            }
            Shortcut::Tab(usize::from(byte - b'1'))
        }
        _ => return None,
    })
}

/// Option is treated as Meta (Alt), as most terminal users expect. Returns
/// `None` for keystrokes that should be left to text input (or to app-level
/// bindings such as Cmd+Q).
pub fn translate(keystroke: &Keystroke) -> Option<Action> {
    let modifiers = &keystroke.modifiers;
    let key = keystroke.key.to_lowercase();
    if modifiers.platform {
        return shortcut(&key, modifiers.shift).map(Action::Shortcut);
    }
    let mut prefix = String::new();
    if modifiers.control {
        prefix.push_str("C-");
    }
    if modifiers.alt {
        prefix.push_str("M-");
    }
    if let Some(name) = named(&key) {
        let is_space = key == "space";
        if prefix.is_empty() && is_space && !modifiers.shift {
            return Some(Action::Send(Input::Text(" ".into())));
        }
        let shift = if modifiers.shift && !is_space {
            "S-"
        } else {
            ""
        };
        return Some(Action::Send(Input::Key(format!("{prefix}{shift}{name}"))));
    }
    if !prefix.is_empty() && key.chars().count() == 1 {
        // `key` is the base key without Option/Shift composition (Option+f → "f", not "ƒ").
        return Some(Action::Send(Input::Key(format!("{prefix}{key}"))));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(source: &str) -> Option<Action> {
        translate(&Keystroke::parse(source).expect("valid keystroke"))
    }

    #[test]
    fn named_keys_and_modifiers() {
        assert_eq!(key("enter"), Some(Action::Send(Input::Key("Enter".into()))));
        assert_eq!(
            key("shift-tab"),
            Some(Action::Send(Input::Key("S-Tab".into())))
        );
        assert_eq!(
            key("ctrl-up"),
            Some(Action::Send(Input::Key("C-Up".into())))
        );
        assert_eq!(key("space"), Some(Action::Send(Input::Text(" ".into()))));
        assert_eq!(
            key("ctrl-space"),
            Some(Action::Send(Input::Key("C-Space".into())))
        );
    }

    #[test]
    fn control_and_meta_characters() {
        assert_eq!(key("ctrl-c"), Some(Action::Send(Input::Key("C-c".into()))));
        assert_eq!(key("alt-f"), Some(Action::Send(Input::Key("M-f".into()))));
        assert_eq!(
            key("ctrl-alt-x"),
            Some(Action::Send(Input::Key("C-M-x".into())))
        );
    }

    #[test]
    fn plain_characters_go_to_text_input() {
        assert_eq!(key("a"), None);
        assert_eq!(key("shift-a"), None);
    }

    #[test]
    fn cmd_shortcuts() {
        assert_eq!(key("cmd-t"), Some(Action::Shortcut(Shortcut::NewTab)));
        assert_eq!(
            key("cmd-shift-d"),
            Some(Action::Shortcut(Shortcut::SplitDown))
        );
        assert_eq!(key("cmd-3"), Some(Action::Shortcut(Shortcut::Tab(2))));
        assert_eq!(
            key("cmd-shift-]"),
            Some(Action::Shortcut(Shortcut::NextTab))
        );
        assert_eq!(
            key("cmd-pageup"),
            Some(Action::Shortcut(Shortcut::ScrollPageUp))
        );
        assert_eq!(key("cmd-c"), Some(Action::Shortcut(Shortcut::Copy)));
        assert_eq!(
            key("cmd-b"),
            Some(Action::Shortcut(Shortcut::ToggleSidebar))
        );
        assert_eq!(key("cmd-q"), None);
    }
}

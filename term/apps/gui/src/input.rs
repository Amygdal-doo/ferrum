//! winit keyboard events → FerrumTerm inputs or app shortcuts (Cmd+…).

use ferrumterm_protocol::Input;
use winit::event::KeyEvent;
use winit::keyboard::{Key, ModifiersState, NamedKey};
use winit::platform::modifier_supplement::KeyEventExtModifierSupplement;

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
}

pub enum Action {
    Send(Input),
    Shortcut(Shortcut),
}

fn named(k: NamedKey) -> Option<&'static str> {
    Some(match k {
        NamedKey::Enter => "Enter",
        NamedKey::Tab => "Tab",
        NamedKey::Escape => "Escape",
        NamedKey::Backspace => "BSpace",
        NamedKey::Space => "Space",
        NamedKey::ArrowUp => "Up",
        NamedKey::ArrowDown => "Down",
        NamedKey::ArrowLeft => "Left",
        NamedKey::ArrowRight => "Right",
        NamedKey::Home => "Home",
        NamedKey::End => "End",
        NamedKey::PageUp => "PageUp",
        NamedKey::PageDown => "PageDown",
        NamedKey::Insert => "Insert",
        NamedKey::Delete => "Delete",
        NamedKey::F1 => "F1",
        NamedKey::F2 => "F2",
        NamedKey::F3 => "F3",
        NamedKey::F4 => "F4",
        NamedKey::F5 => "F5",
        NamedKey::F6 => "F6",
        NamedKey::F7 => "F7",
        NamedKey::F8 => "F8",
        NamedKey::F9 => "F9",
        NamedKey::F10 => "F10",
        NamedKey::F11 => "F11",
        NamedKey::F12 => "F12",
        _ => return None,
    })
}

fn shortcut(key: &Key, m: ModifiersState) -> Option<Shortcut> {
    let shift = m.shift_key();
    Some(match key {
        Key::Character(c) => match (c.to_lowercase().as_str(), shift) {
            ("t", false) => Shortcut::NewTab,
            ("n", false) => Shortcut::NewSession,
            ("d", false) => Shortcut::SplitRight,
            ("d", true) => Shortcut::SplitDown,
            ("w", _) => Shortcut::CloseBlock,
            ("]", false) => Shortcut::NextBlock,
            ("[", false) => Shortcut::PrevBlock,
            ("]" | "}", true) => Shortcut::NextTab,
            ("[" | "{", true) => Shortcut::PrevTab,
            ("v", _) => Shortcut::Paste,
            ("c", true) => Shortcut::CopyScreen,
            ("=" | "+", _) => Shortcut::FontBigger,
            ("-", _) => Shortcut::FontSmaller,
            ("0", false) => Shortcut::FontReset,
            ("s", true) => Shortcut::NextSession,
            (d, false) if d.len() == 1 && d.as_bytes()[0].is_ascii_digit() && d != "0" => {
                Shortcut::Tab(usize::from(d.as_bytes()[0] - b'1'))
            }
            _ => return None,
        },
        Key::Named(NamedKey::PageUp) => Shortcut::ScrollPageUp,
        Key::Named(NamedKey::PageDown) => Shortcut::ScrollPageDown,
        _ => return None,
    })
}

/// Option is treated as Meta (Alt), as most terminal users expect.
pub fn translate(event: &KeyEvent, m: ModifiersState) -> Option<Action> {
    if m.super_key() {
        return shortcut(&event.logical_key, m).map(Action::Shortcut);
    }
    let mut prefix = String::new();
    if m.control_key() {
        prefix.push_str("C-");
    }
    if m.alt_key() {
        prefix.push_str("M-");
    }
    match &event.logical_key {
        Key::Named(k) => {
            let name = named(*k)?;
            let shift = if m.shift_key() && *k != NamedKey::Space {
                "S-"
            } else {
                ""
            };
            if prefix.is_empty() && shift.is_empty() && *k == NamedKey::Space {
                return Some(Action::Send(Input::Text(" ".into())));
            }
            Some(Action::Send(Input::Key(format!("{prefix}{shift}{name}"))))
        }
        Key::Character(_) if !prefix.is_empty() => {
            // Base key without Option/Shift composition (Option+f → "f", not "ƒ").
            let base = match event.key_without_modifiers() {
                Key::Character(s) => s.to_string(),
                _ => return None,
            };
            Some(Action::Send(Input::Key(format!("{prefix}{base}"))))
        }
        _ => event
            .text
            .as_ref()
            .filter(|t| !t.is_empty())
            .map(|t| Action::Send(Input::Text(t.to_string()))),
    }
}

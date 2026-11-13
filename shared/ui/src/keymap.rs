//! Shortcuts recorded in Settings › Shortcuts, kept per app in
//! `~/.ferrum/<binary>/keymap.json` and bound on top of the app's own
//! bindings at start (later bindings win in GPUI), so a recorded key takes
//! priority over the built-in one.

use std::path::PathBuf;
use std::rc::Rc;

use gpui::{App, DummyKeyboardMapper, KeyBinding, KeyBindingContextPredicate};
use serde::{Deserialize, Serialize};

use crate::app_info::app_info;
use crate::prefs;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Override {
    /// GPUI action name, e.g. `ferrum_term::NewTab`.
    pub action: String,
    /// Keystrokes in GPUI syntax (`cmd-shift-t`); empty unbinds nothing but
    /// is kept so "Reset" can tell it apart.
    pub keys: String,
    /// Key context of the binding it replaces, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overrides {
    pub bindings: Vec<Override>,
}

impl Overrides {
    pub fn get(&self, action: &str) -> Option<&Override> {
        self.bindings.iter().find(|o| o.action == action)
    }

    pub fn set(&mut self, value: Override) {
        self.bindings.retain(|o| o.action != value.action);
        self.bindings.push(value);
    }

    pub fn remove(&mut self, action: &str) {
        self.bindings.retain(|o| o.action != action);
    }
}

pub fn file(cx: &App) -> Option<PathBuf> {
    prefs::path(&format!("{}/keymap.json", app_info(cx).binary))
}

pub fn load(cx: &App) -> Overrides {
    file(cx).map(|f| prefs::read(&f)).unwrap_or_default()
}

pub fn save(overrides: &Overrides, cx: &App) {
    if let Some(file) = file(cx)
        && let Err(error) = prefs::write(&file, overrides)
    {
        eprintln!("ferrum: could not save {}: {error}", file.display());
    }
}

/// Builds the binding for `value`, if the action and keys are valid here.
pub fn binding(value: &Override, cx: &App) -> Option<KeyBinding> {
    if value.keys.trim().is_empty() {
        return None;
    }
    let action = cx.build_action(&value.action, None).ok()?;
    let predicate = match &value.context {
        Some(context) => Some(Rc::new(KeyBindingContextPredicate::parse(context).ok()?)),
        None => None,
    };
    KeyBinding::load(
        &value.keys,
        action,
        predicate,
        false,
        None,
        &DummyKeyboardMapper,
    )
    .ok()
}

/// Binds every saved override. Called by `ferrum_ui::run` after the app's
/// own bindings are registered.
pub(crate) fn apply_saved(cx: &mut App) {
    let bindings: Vec<KeyBinding> = load(cx)
        .bindings
        .iter()
        .filter_map(|o| binding(o, cx))
        .collect();
    cx.bind_keys(bindings);
}

/// Records `value`, binds it now and saves it.
pub fn record(value: Override, cx: &mut App) {
    if let Some(binding) = binding(&value, cx) {
        cx.bind_keys([binding]);
    }
    let mut overrides = load(cx);
    overrides.set(value);
    save(&overrides, cx);
}

/// Forgets the override for `action`. The app's own binding returns after
/// a restart (GPUI cannot unbind a single key while running).
pub fn reset(action: &str, cx: &mut App) {
    let mut overrides = load(cx);
    overrides.remove(action);
    save(&overrides, cx);
}

/// `ferrum_term::NewTab` → ("Ferrum Term", "New Tab").
pub fn describe(action: &str) -> (String, String) {
    let (namespace, name) = action.rsplit_once("::").unwrap_or(("", action));
    let namespace = namespace.rsplit("::").next().unwrap_or(namespace);
    (
        title_case(&namespace.replace(['_', '-'], " ")),
        split_camel(name),
    )
}

fn title_case(text: &str) -> String {
    text.split_whitespace()
        .map(|w| {
            let mut c = w.chars();
            c.next()
                .map(|f| f.to_uppercase().chain(c).collect::<String>())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn split_camel(name: &str) -> String {
    let mut out = String::new();
    let chars: Vec<char> = name.chars().collect();
    for (i, &c) in chars.iter().enumerate() {
        let boundary = i > 0
            && c.is_uppercase()
            && (chars[i - 1].is_lowercase() || chars.get(i + 1).is_some_and(|n| n.is_lowercase()));
        if boundary || (i > 0 && c.is_ascii_digit() && !chars[i - 1].is_ascii_digit()) {
            out.push(' ');
        }
        out.push(c);
    }
    out.replace('_', " ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_action_names() {
        assert_eq!(
            describe("ferrum_term::NewTab"),
            ("Ferrum Term".into(), "New Tab".into())
        );
        assert_eq!(describe("menu::SelectNext").1, "Select Next");
        assert_eq!(describe("term::ActivateTab1").1, "Activate Tab 1");
        assert_eq!(describe("x::OpenURLInBrowser").1, "Open URL In Browser");
        assert_eq!(describe("Quit"), (String::new(), "Quit".into()));
    }

    #[test]
    fn overrides_replace_per_action() {
        let mut o = Overrides::default();
        let ov = |keys: &str| Override {
            action: "a::B".into(),
            keys: keys.into(),
            context: None,
        };
        o.set(ov("cmd-1"));
        o.set(ov("cmd-2"));
        assert_eq!(o.bindings.len(), 1);
        assert_eq!(o.get("a::B").map(|o| o.keys.as_str()), Some("cmd-2"));
        o.remove("a::B");
        assert!(o.bindings.is_empty());
    }
}

//! Settings › Shortcuts: every key binding of the app, filterable by name
//! or key, with recording of new keys (saved by [`crate::keymap`]).

use std::cell::RefCell;
use std::collections::BTreeMap;

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, Hsla, IntoElement, KeyBinding,
    ParentElement, Render, Styled, Subscription, Window, div, prelude::*,
};

use super::widgets::{Row, Section, key_caps};
use super::{SettingsPage, tint};
use crate::components::{
    Button, ButtonCommon, ButtonStyle, Clickable, IconName, format_keystroke, h_flex,
};
use crate::keymap::{self, Override};
use crate::text_input::{TextInput, TextInputEvent};

#[derive(Default)]
pub(super) struct ShortcutsPage {
    view: RefCell<Option<Entity<ShortcutsView>>>,
}

impl SettingsPage for ShortcutsPage {
    fn id(&self) -> &'static str {
        "shortcuts"
    }
    fn title(&self) -> &'static str {
        "Shortcuts"
    }
    fn icon(&self) -> IconName {
        IconName::Hash
    }
    fn tint(&self) -> Hsla {
        tint(0x8b8b90)
    }
    fn keywords(&self) -> &'static [&'static str] {
        &["keys", "keyboard", "keymap", "bindings", "hotkeys"]
    }

    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement {
        let view = self
            .view
            .borrow_mut()
            .get_or_insert_with(|| cx.new(|cx| ShortcutsView::new(window, cx)))
            .clone();
        view.into_any_element()
    }
}

/// One action and the key that performs it.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    action: String,
    group: String,
    label: String,
    /// GPUI syntax, for matching a typed filter like `cmd-k`.
    keys: String,
    /// Key caps for display, e.g. `⇧ ⌘ P`.
    caps: String,
    context: Option<String>,
    recorded: bool,
}

/// Namespaces of plumbing actions not worth listing.
const HIDDEN: &[&str] = &["menu", "text_input", "zed", "gpui"];

fn caps(binding: &KeyBinding) -> String {
    binding
        .keystrokes()
        .iter()
        .map(|k| {
            let text = format_keystroke(k.inner());
            // Split glyph modifiers from the key so each gets its own cap.
            let mut parts: Vec<String> = Vec::new();
            let mut rest = text.as_str();
            while let Some(c) = rest.chars().next().filter(|c| "⌃⌥⇧⌘".contains(*c)) {
                parts.push(c.to_string());
                rest = &rest[c.len_utf8()..];
            }
            parts.push(rest.trim_start_matches('-').to_owned());
            parts.join(" ")
        })
        .collect::<Vec<_>>()
        .join("  ")
}

fn entries(cx: &App) -> Vec<Entry> {
    let overrides = keymap::load(cx);
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    // Later bindings win, so the last one per action is what the key does.
    let mut by_action: BTreeMap<String, Entry> = BTreeMap::new();
    for binding in keymap.bindings() {
        let action = binding.action().name().to_owned();
        let (group, label) = keymap::describe(&action);
        let namespace = action.rsplit_once("::").map_or("", |(n, _)| n);
        if HIDDEN.contains(&namespace) || label.is_empty() {
            continue;
        }
        let keys = binding
            .keystrokes()
            .iter()
            .map(|k| k.inner().unparse())
            .collect::<Vec<_>>()
            .join(" ");
        by_action.insert(
            action.clone(),
            Entry {
                recorded: overrides.get(&action).is_some_and(|o| o.keys == keys),
                group: if group.is_empty() {
                    "App".into()
                } else {
                    group
                },
                label,
                keys,
                caps: caps(binding),
                context: binding.predicate().map(|p| p.to_string()),
                action,
            },
        );
    }
    let mut list: Vec<Entry> = by_action.into_values().collect();
    list.sort_by(|a, b| (&a.group, &a.label).cmp(&(&b.group, &b.label)));
    list
}

fn matches(entry: &Entry, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || entry.label.to_lowercase().contains(&query)
        || entry.group.to_lowercase().contains(&query)
        || entry
            .keys
            .replace('+', "-")
            .contains(&query.replace('+', "-"))
}

pub(crate) struct ShortcutsView {
    filter: Entity<TextInput>,
    recording: Option<Entry>,
    _subscriptions: Vec<Subscription>,
}

impl ShortcutsView {
    fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| TextInput::new(window, cx).placeholder("Filter Shortcuts…"));
        let changed = cx.subscribe(&filter, |_, _, event, cx| {
            if *event == TextInputEvent::Changed {
                cx.notify();
            }
        });
        let this = cx.weak_entity();
        let intercept = cx.intercept_keystrokes(move |event, _, cx| {
            let _ = this.update(cx, |view, cx| view.capture(&event.keystroke, cx));
        });
        Self {
            filter,
            recording: None,
            _subscriptions: vec![changed, intercept],
        }
    }

    fn capture(&mut self, keystroke: &gpui::Keystroke, cx: &mut Context<Self>) {
        let Some(entry) = self.recording.clone() else {
            return;
        };
        let key = keystroke.key.as_str();
        if matches!(
            key,
            "shift" | "control" | "alt" | "platform" | "function" | ""
        ) {
            return;
        }
        cx.stop_propagation();
        self.recording = None;
        let m = &keystroke.modifiers;
        let bare = !(m.control || m.alt || m.platform || m.function);
        if key == "escape" && bare {
            cx.notify();
            return;
        }
        keymap::record(
            Override {
                action: entry.action,
                keys: keystroke.unparse(),
                context: entry.context,
            },
            cx,
        );
        cx.notify();
    }
}

impl Render for ShortcutsView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let query = self.filter.read(cx).text().to_owned();
        let list: Vec<Entry> = entries(cx)
            .into_iter()
            .filter(|e| matches(e, &query))
            .collect();
        let mut groups: Vec<(String, Vec<Entry>)> = Vec::new();
        for entry in list {
            match groups.last_mut() {
                Some((group, items)) if *group == entry.group => items.push(entry),
                _ => groups.push((entry.group.clone(), vec![entry])),
            }
        }
        let recording = self.recording.as_ref().map(|e| e.action.clone());
        v_flex_gap()
            .child(self.filter.clone())
            .child(Section::untitled().footnote(
                "A shortcut recorded here takes priority over the built-in one. Search for a \
                     key, such as cmd-k, to see everything bound to it. Press Esc to cancel \
                     recording.",
            ))
            .children(groups.into_iter().enumerate().map(|(gi, (group, items))| {
                Section::new(group).rows(items.into_iter().enumerate().map(|(i, entry)| {
                    let is_recording = recording.as_deref() == Some(entry.action.as_str());
                    let action = entry.action.clone();
                    let target = entry.clone();
                    let control = h_flex()
                        .gap_2()
                        .when(entry.recorded, |this| {
                            this.child(
                                Button::new(("reset", gi * 1000 + i), "Reset")
                                    .style(ButtonStyle::Subtle)
                                    .on_click(move |_, _, cx| {
                                        keymap::reset(&action, cx);
                                        cx.refresh_windows();
                                    }),
                            )
                        })
                        .child(
                            div()
                                .id(("record", gi * 1000 + i))
                                .cursor_pointer()
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.recording = Some(target.clone());
                                    cx.notify();
                                }))
                                .child(if is_recording {
                                    Button::new(("recording", gi * 1000 + i), "Type a shortcut…")
                                        .style(ButtonStyle::Filled)
                                        .into_any_element()
                                } else {
                                    key_caps(&entry.caps, cx).into_any_element()
                                }),
                        );
                    Row::new(("shortcut", gi * 1000 + i), entry.label.clone()).control(control)
                }))
            }))
    }
}

fn v_flex_gap() -> gpui::Div {
    crate::components::v_flex().w_full().gap_6()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(label: &str, keys: &str) -> Entry {
        Entry {
            action: format!("a::{label}"),
            group: "Term".into(),
            label: label.into(),
            keys: keys.into(),
            caps: String::new(),
            context: None,
            recorded: false,
        }
    }

    #[test]
    fn filters_by_name_group_or_key() {
        let e = entry("New Tab", "cmd-t");
        assert!(matches(&e, ""));
        assert!(matches(&e, "new"));
        assert!(matches(&e, "term"));
        assert!(matches(&e, "cmd+t"));
        assert!(!matches(&e, "split"));
    }
}

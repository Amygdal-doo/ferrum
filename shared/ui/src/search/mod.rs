//! The ways to search, the same in every Ferrum app:
//!
//! - **Search Everywhere** (Shift twice): the app's items, symbols, actions,
//!   settings and text by name, in one modal. Tab / Shift-Tab move between
//!   its tabs.
//! - **Go to** (cmd-p / ctrl-p): the app's items only (files in FerrumIDE,
//!   requests in FerrumAPI, containers in FerrumContainers).
//! - **Find in files** (cmd-shift-f / ctrl-shift-f): text across everything
//!   the app holds, with case, whole word, regex and a file mask, in a modal.
//! - **Find in the current view** (cmd-f / ctrl-f): each app's own find bar.
//!
//! An app gives a [`SearchModal`] its items and, for text, a
//! [`TextSearcher`]; the modal does the rest and emits [`SearchEvent`]s.
//! Render it with [`search_overlay`] as the root view's last child, and
//! feed modifier and key events to a [`DoubleShift`] to open Search
//! Everywhere.

mod fuzzy;
mod modal;
mod text;

use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use gpui::{App, KeyBinding, Modifiers, SharedString};

use crate::components::IconName;

pub use fuzzy::{FuzzyMatch, fuzzy_filter, fuzzy_match};
pub use modal::{SearchModal, search_overlay};
pub use text::TextMatcher;

gpui::actions!(
    ferrum_search,
    [
        /// Search items, symbols, actions, settings and text (Shift twice).
        SearchEverywhere,
        /// Open one of the app's items by name (cmd-p).
        GoToItem,
        /// Search text across everything the app holds (cmd-shift-f).
        FindInFiles,
        /// Find in the current view (cmd-f).
        FindInView,
        /// Next tab of the search modal.
        NextTab,
        /// Previous tab of the search modal.
        PreviousTab,
    ]
);

/// Key context of the search modal.
pub const CONTEXT: &str = "FerrumSearch";

/// Binds cmd-p / cmd-shift-f / cmd-f (ctrl on Linux and Windows) app-wide,
/// and the modal's own keys. Apps that already use one of these keys for
/// something else bind theirs after this, in a deeper context.
pub fn bind_keys(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-p", GoToItem, None),
        KeyBinding::new("secondary-shift-f", FindInFiles, None),
        KeyBinding::new("secondary-f", FindInView, None),
        // Ahead of an app's own Escape in its root context.
        KeyBinding::new("escape", crate::menu::Cancel, Some(CONTEXT)),
        KeyBinding::new("tab", NextTab, Some(CONTEXT)),
        KeyBinding::new("shift-tab", PreviousTab, Some(CONTEXT)),
    ]);
}

/// What the modal is for; decides its tabs and placeholder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SearchMode {
    /// Every tab, starting at All.
    Everywhere,
    /// The items tab only.
    GoTo,
    /// Text across everything, with options.
    FindInFiles,
}

/// A tab of Search Everywhere. Apps name [`SearchCategory::Items`] for what
/// they hold ("Files", "Requests", "Containers").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SearchCategory {
    Items,
    Symbols,
    Actions,
    Settings,
    Text,
}

impl SearchCategory {
    pub const ALL: [SearchCategory; 5] = [
        Self::Items,
        Self::Symbols,
        Self::Actions,
        Self::Settings,
        Self::Text,
    ];

    pub fn default_label(self) -> &'static str {
        match self {
            Self::Items => "Items",
            Self::Symbols => "Symbols",
            Self::Actions => "Actions",
            Self::Settings => "Settings",
            Self::Text => "Text",
        }
    }
}

/// One thing the modal can find by name.
#[derive(Debug, Clone)]
pub struct SearchItem {
    pub category: SearchCategory,
    /// What is matched and shown.
    pub title: SharedString,
    /// Secondary text on the right (folder, kind, container image).
    pub detail: SharedString,
    /// Shortcut shown for actions (`⌘⇧P`).
    pub keys: Option<SharedString>,
    pub icon: Option<IconName>,
    /// The app's own handle for the item, handed back in [`SearchEvent::Open`].
    pub id: SharedString,
}

impl SearchItem {
    pub fn new(
        category: SearchCategory,
        title: impl Into<SharedString>,
        id: impl Into<SharedString>,
    ) -> Self {
        Self {
            category,
            title: title.into(),
            detail: SharedString::default(),
            keys: None,
            icon: None,
            id: id.into(),
        }
    }

    #[must_use]
    pub fn detail(mut self, detail: impl Into<SharedString>) -> Self {
        self.detail = detail.into();
        self
    }

    #[must_use]
    pub fn keys(mut self, keys: impl Into<SharedString>) -> Self {
        self.keys = Some(keys.into());
        self
    }

    #[must_use]
    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }
}

/// A text search: the query and its options.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TextQuery {
    pub text: String,
    pub case_sensitive: bool,
    pub whole_word: bool,
    pub regex: bool,
    /// Comma-separated globs; `!` excludes (`*.rs, !*_test.rs`). Empty: all.
    pub mask: String,
}

/// One line found by a text search.
#[derive(Debug, Clone)]
pub struct TextHit {
    /// The app's handle for where the line is (a path, a request id).
    pub source: SharedString,
    /// Where it is, as shown (`src/main.rs`, `GET /users`).
    pub source_label: SharedString,
    /// 0-based line.
    pub line: usize,
    /// 0-based byte column of the match in the line.
    pub column: usize,
    /// The line (trimmed to a sensible length).
    pub preview: SharedString,
    /// The match inside `preview`, in bytes.
    pub range: Range<usize>,
}

/// Runs a text search on a background thread. It should return early when
/// the flag turns true (a newer search started) and stop at `limit` hits.
pub type TextSearcher =
    Arc<dyn Fn(&TextQuery, &AtomicBool, usize) -> anyhow::Result<Vec<TextHit>> + Send + Sync>;

pub enum SearchEvent {
    /// An item was chosen.
    Open(SearchItem),
    /// A text hit was chosen.
    OpenHit(TextHit),
    Dismissed,
}

/// The Settings window's pages as [`SearchCategory::Settings`] items: each
/// page by title, and each of its keywords ("font size (Interface)").
/// [`open_common`] opens them.
pub fn settings_items(cx: &App) -> Vec<SearchItem> {
    let mut items = Vec::new();
    for page in crate::settings::all_pages(cx) {
        let id = format!("{SETTINGS_PREFIX}{}", page.id());
        items.push(
            SearchItem::new(SearchCategory::Settings, page.title(), id.clone())
                .detail("Settings")
                .icon(page.icon()),
        );
        for keyword in page.keywords() {
            items.push(
                SearchItem::new(SearchCategory::Settings, *keyword, id.clone())
                    .detail(format!("Settings › {}", page.title()))
                    .icon(page.icon()),
            );
        }
    }
    items
}

const SETTINGS_PREFIX: &str = "settings:";

/// Handles the items every app shares (Settings pages); false for the
/// app's own.
pub fn open_common(item: &SearchItem, cx: &mut App) -> bool {
    match item.id.strip_prefix(SETTINGS_PREFIX) {
        Some(page) if item.category == SearchCategory::Settings => {
            crate::settings::open_settings(Some(page), cx);
            true
        }
        _ => false,
    }
}

/// Tells a press of Shift twice (no other key in between, within 400 ms)
/// from Shift used for typing.
#[derive(Debug, Default)]
pub struct DoubleShift {
    /// Shift is down on its own.
    down: bool,
    /// When Shift was last released after being pressed on its own.
    last_tap: Option<Instant>,
}

impl DoubleShift {
    const WINDOW: Duration = Duration::from_millis(400);

    /// Feed every modifier change; true when the second tap completes.
    pub fn modifiers_changed(&mut self, modifiers: &Modifiers) -> bool {
        self.modifiers_changed_at(modifiers, Instant::now())
    }

    fn modifiers_changed_at(&mut self, modifiers: &Modifiers, now: Instant) -> bool {
        if *modifiers == Modifiers::shift() {
            self.down = true;
            return false;
        }
        if !self.down {
            self.last_tap = None;
            return false;
        }
        self.down = false;
        if modifiers.modified() {
            // Another modifier joined Shift: not a tap.
            self.last_tap = None;
            return false;
        }
        match self.last_tap.take() {
            Some(at) if now.duration_since(at) <= Self::WINDOW => true,
            _ => {
                self.last_tap = Some(now);
                false
            }
        }
    }

    /// Feed every key press (capture phase): typing between taps cancels.
    pub fn key_down(&mut self) {
        self.down = false;
        self.last_tap = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_quick_shift_taps_trigger() {
        let mut d = DoubleShift::default();
        let t = Instant::now();
        assert!(!d.modifiers_changed_at(&Modifiers::shift(), t));
        assert!(!d.modifiers_changed_at(&Modifiers::none(), t));
        assert!(!d.modifiers_changed_at(&Modifiers::shift(), t + Duration::from_millis(100)));
        assert!(d.modifiers_changed_at(&Modifiers::none(), t + Duration::from_millis(150)));
        // A third tap starts over.
        assert!(!d.modifiers_changed_at(&Modifiers::shift(), t + Duration::from_millis(200)));
        assert!(!d.modifiers_changed_at(&Modifiers::none(), t + Duration::from_millis(250)));
    }

    #[test]
    fn slow_taps_typing_or_chords_do_not_trigger() {
        let mut d = DoubleShift::default();
        let t = Instant::now();
        d.modifiers_changed_at(&Modifiers::shift(), t);
        d.modifiers_changed_at(&Modifiers::none(), t);
        d.modifiers_changed_at(&Modifiers::shift(), t + Duration::from_secs(1));
        assert!(!d.modifiers_changed_at(&Modifiers::none(), t + Duration::from_secs(1)));

        let mut d = DoubleShift::default();
        d.modifiers_changed_at(&Modifiers::shift(), t);
        d.key_down(); // Shift-A
        d.modifiers_changed_at(&Modifiers::none(), t);
        d.modifiers_changed_at(&Modifiers::shift(), t);
        assert!(!d.modifiers_changed_at(&Modifiers::none(), t));

        let mut d = DoubleShift::default();
        d.modifiers_changed_at(&Modifiers::shift(), t);
        d.modifiers_changed_at(&Modifiers::none(), t);
        d.modifiers_changed_at(&Modifiers::shift(), t);
        let cmd_shift = Modifiers {
            platform: true,
            shift: true,
            ..Modifiers::none()
        };
        assert!(!d.modifiers_changed_at(&cmd_shift, t));
    }
}

//! The suite-wide look, chosen in onboarding or Settings › Interface and
//! kept in `~/.ferrum/appearance.json`: colour scheme (Auto follows the
//! system), the dark and light theme, toolbar density and text size.
//!
//! Every Ferrum app applies it at start and picks up changes another app
//! makes while it runs, so the whole suite changes together.

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime};

use gpui::{App, Global, Pixels, WindowAppearance, px};
use serde::{Deserialize, Serialize};

use crate::prefs;
use crate::theme::presets::{self, ThemePreset};
use crate::theme::{Appearance, GlobalTheme};

/// Light, dark, or whatever the system uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ColorScheme {
    #[default]
    Auto,
    Light,
    Dark,
}

/// Height of the toolbars and bars at the top of every window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ToolbarStyle {
    #[default]
    Comfortable,
    Compact,
}

impl ToolbarStyle {
    pub fn height(self) -> Pixels {
        match self {
            Self::Comfortable => px(40.),
            Self::Compact => px(32.),
        }
    }
}

/// Interface text size (the window's rem size).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TextSize {
    Small,
    #[default]
    Default,
    Large,
}

impl TextSize {
    pub fn rem(self) -> Pixels {
        match self {
            Self::Small => px(15.),
            Self::Default => px(16.),
            Self::Large => px(17.5),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AppearancePrefs {
    pub scheme: ColorScheme,
    /// Preset id used while the scheme is dark.
    pub dark_theme: String,
    /// Preset id used while the scheme is light.
    pub light_theme: String,
    pub toolbar: ToolbarStyle,
    pub text_size: TextSize,
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            dark_theme: presets::default_id(Appearance::Dark).into(),
            light_theme: presets::default_id(Appearance::Light).into(),
            toolbar: ToolbarStyle::default(),
            text_size: TextSize::default(),
        }
    }
}

impl AppearancePrefs {
    /// The appearance in effect, given what the system uses.
    pub fn effective(&self, system: WindowAppearance) -> Appearance {
        match self.scheme {
            ColorScheme::Light => Appearance::Light,
            ColorScheme::Dark => Appearance::Dark,
            ColorScheme::Auto => match system {
                WindowAppearance::Light | WindowAppearance::VibrantLight => Appearance::Light,
                WindowAppearance::Dark | WindowAppearance::VibrantDark => Appearance::Dark,
            },
        }
    }

    /// The theme id chosen for `appearance`.
    pub fn theme_id(&self, appearance: Appearance) -> &str {
        match appearance {
            Appearance::Dark => &self.dark_theme,
            Appearance::Light => &self.light_theme,
        }
    }

    /// Picks `id` for its own appearance; switches a fixed scheme over to
    /// it so the choice is visible at once.
    pub fn choose_theme(&mut self, id: &str, appearance: Appearance) {
        match appearance {
            Appearance::Dark => self.dark_theme = id.into(),
            Appearance::Light => self.light_theme = id.into(),
        }
        if self.scheme != ColorScheme::Auto {
            self.scheme = match appearance {
                Appearance::Dark => ColorScheme::Dark,
                Appearance::Light => ColorScheme::Light,
            };
        }
    }

    /// The preset for `appearance`, falling back to the default one.
    pub fn preset(&self, appearance: Appearance) -> &'static ThemePreset {
        presets::find(self.theme_id(appearance))
            .filter(|p| p.appearance == appearance)
            .or_else(|| presets::find(presets::default_id(appearance)))
            .unwrap_or(&presets::PRESETS[0])
    }
}

/// A host that renders its own themes (Ferrum Editor) installs this to be
/// told the suite's choice instead of having `ferrum-ui` install a preset.
/// Return `true` when the host applied a theme itself.
pub type ThemeHook = Rc<dyn Fn(&AppearancePrefs, Appearance, &mut App) -> bool>;

struct AppearanceState {
    prefs: AppearancePrefs,
    file: Option<PathBuf>,
    seen: Option<SystemTime>,
    hook: Option<ThemeHook>,
    /// The last appearance applied, to skip identical re-applies.
    applied: Option<(Appearance, String)>,
}

impl Global for AppearanceState {}

fn file() -> Option<PathBuf> {
    prefs::path("appearance.json")
}

/// Loads the saved appearance and applies it. Called by `ferrum_ui::run`.
pub(crate) fn init(cx: &mut App) {
    let file = file();
    let prefs = file.as_deref().map(prefs::read).unwrap_or_default();
    let seen = file.as_deref().and_then(prefs::modified);
    cx.set_global(AppearanceState {
        prefs,
        file,
        seen,
        hook: None,
        applied: None,
    });
    apply(cx);
}

/// The current appearance preferences.
pub fn current(cx: &App) -> AppearancePrefs {
    cx.try_global::<AppearanceState>()
        .map(|s| s.prefs.clone())
        .unwrap_or_default()
}

/// The appearance in effect right now.
pub fn effective(cx: &App) -> Appearance {
    current(cx).effective(cx.window_appearance())
}

/// Saves `prefs` for every Ferrum app and applies them here.
pub fn set(prefs: AppearancePrefs, cx: &mut App) {
    if !cx.has_global::<AppearanceState>() {
        init(cx);
    }
    let state = cx.global_mut::<AppearanceState>();
    state.prefs = prefs;
    if let Some(file) = state.file.clone() {
        if let Err(error) = prefs::write(&file, &state.prefs) {
            eprintln!("ferrum: could not save {}: {error}", file.display());
        }
        state.seen = prefs::modified(&file);
    }
    apply(cx);
}

/// Changes the preferences with `f`, then saves and applies them.
pub fn update(cx: &mut App, f: impl FnOnce(&mut AppearancePrefs)) {
    let mut prefs = current(cx);
    f(&mut prefs);
    set(prefs, cx);
}

/// See [`ThemeHook`]. Re-applies the current choice through the hook.
pub fn set_theme_hook(hook: ThemeHook, cx: &mut App) {
    if !cx.has_global::<AppearanceState>() {
        init(cx);
    }
    let state = cx.global_mut::<AppearanceState>();
    state.hook = Some(hook);
    state.applied = None;
    apply(cx);
}

/// Installs the theme for the current preferences and system appearance.
pub fn apply(cx: &mut App) {
    let Some(state) = cx.try_global::<AppearanceState>() else {
        return;
    };
    let prefs = state.prefs.clone();
    let hook = state.hook.clone();
    let appearance = prefs.effective(cx.window_appearance());
    let key = (appearance, prefs.theme_id(appearance).to_owned());
    let unchanged = state.applied.as_ref() == Some(&key);
    cx.global_mut::<AppearanceState>().applied = Some(key);
    if !unchanged {
        let handled = hook.is_some_and(|hook| hook(&prefs, appearance, cx));
        if !handled {
            GlobalTheme::update_theme(cx, Arc::new(prefs.preset(appearance).theme()));
        }
    }
    cx.refresh_windows();
}

/// Re-reads the file when another app changed it; `true` if it did.
fn reload_if_changed(cx: &mut App) -> bool {
    let Some(state) = cx.try_global::<AppearanceState>() else {
        return false;
    };
    let Some(file) = state.file.clone() else {
        return false;
    };
    let modified = prefs::modified(&file);
    if modified == state.seen {
        return false;
    }
    let prefs = prefs::read(&file);
    let state = cx.global_mut::<AppearanceState>();
    state.seen = modified;
    if state.prefs == prefs {
        return false;
    }
    state.prefs = prefs;
    true
}

/// Follows the system appearance (for Auto) and changes saved by other
/// Ferrum apps. Called once by `ferrum_ui::run`.
pub(crate) fn watch(cx: &mut App) {
    cx.spawn(async move |cx| {
        let mut system = None;
        loop {
            cx.background_executor()
                .timer(Duration::from_millis(1500))
                .await;
            cx.update(|cx| {
                let now = cx.window_appearance();
                let changed = reload_if_changed(cx) || system.is_some_and(|s| s != now);
                system = Some(now);
                if changed {
                    apply(cx);
                }
            });
        }
    })
    .detach();
}

/// Toolbar height for the current preferences.
pub fn toolbar_height(cx: &App) -> Pixels {
    cx.try_global::<AppearanceState>()
        .map(|s| s.prefs.toolbar)
        .unwrap_or_default()
        .height()
}

/// Rem size for the current preferences.
pub fn rem_size(cx: &App) -> Pixels {
    cx.try_global::<AppearanceState>()
        .map(|s| s.prefs.text_size)
        .unwrap_or_default()
        .rem()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_follows_the_system() {
        let prefs = AppearancePrefs {
            scheme: ColorScheme::Auto,
            ..AppearancePrefs::default()
        };
        assert_eq!(prefs.effective(WindowAppearance::Light), Appearance::Light);
        assert_eq!(
            prefs.effective(WindowAppearance::VibrantDark),
            Appearance::Dark
        );
        let dark = AppearancePrefs::default();
        assert_eq!(dark.effective(WindowAppearance::Light), Appearance::Dark);
    }

    #[test]
    fn choosing_a_theme_switches_a_fixed_scheme() {
        let mut prefs = AppearancePrefs::default();
        prefs.choose_theme("glacier", Appearance::Light);
        assert_eq!(prefs.scheme, ColorScheme::Light);
        assert_eq!(prefs.light_theme, "glacier");
        prefs.scheme = ColorScheme::Auto;
        prefs.choose_theme("plum-ink", Appearance::Dark);
        assert_eq!(prefs.scheme, ColorScheme::Auto);
        assert_eq!(prefs.preset(Appearance::Dark).id, "plum-ink");
    }

    #[test]
    fn unknown_or_mismatched_ids_fall_back() {
        let prefs = AppearancePrefs {
            dark_theme: "glacier".into(),
            light_theme: "nope".into(),
            ..AppearancePrefs::default()
        };
        assert_eq!(prefs.preset(Appearance::Dark).id, "ferrum-dark");
        assert_eq!(prefs.preset(Appearance::Light).id, "ferrum-light");
    }

    #[test]
    fn old_files_without_new_fields_still_load() {
        let prefs: AppearancePrefs = serde_json::from_str(r#"{"scheme":"light"}"#).expect("parse");
        assert_eq!(prefs.scheme, ColorScheme::Light);
        assert_eq!(prefs.toolbar, ToolbarStyle::Comfortable);
    }
}

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

use gpui::{App, Global, Hsla, Pixels, WindowAppearance, WindowBackgroundAppearance, px};
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

/// How window surfaces are drawn: flat colours, a gradient behind
/// translucent panes, or translucent panes over the blurred desktop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceStyle {
    #[default]
    Solid,
    Gradient,
    Glass,
}

impl InterfaceStyle {
    pub const ALL: [Self; 3] = [Self::Solid, Self::Gradient, Self::Glass];

    pub fn label(self) -> &'static str {
        match self {
            Self::Solid => "Solid",
            Self::Gradient => "Gradient",
            Self::Glass => "Glass",
        }
    }

    /// Opacity of pane surfaces (editor, terminal, panels) in this style.
    pub fn pane_alpha(self) -> f32 {
        match self {
            Self::Solid => 1.,
            Self::Gradient => 0.84,
            Self::Glass => 0.72,
        }
    }

    /// Opacity of window chrome (title, tab and status bars) in this style.
    pub fn chrome_alpha(self) -> f32 {
        match self {
            Self::Solid => 1.,
            Self::Gradient => 0.,
            Self::Glass => 0.,
        }
    }
}

/// The window's title rows: on the backdrop (the default), or on a
/// tinted strip like a classic title bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TitleBarStyle {
    #[default]
    Transparent,
    Tinted,
}

/// The tinted title rows' fill, `None` while [`TitleBarStyle::Transparent`];
/// kept here for elements built without an `App`.
static TITLE_BAR_FILL: std::sync::Mutex<Option<Hsla>> = std::sync::Mutex::new(None);

/// The fill of title rows (see [`TitleBarStyle`]); `None`: transparent.
pub fn title_bar_fill() -> Option<Hsla> {
    TITLE_BAR_FILL.lock().ok().and_then(|fill| *fill)
}

/// The current [`ToolbarStyle`], for elements built without an `App`.
static COMPACT: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Whether the interface is set to Compact (smaller title rows, sidebar
/// rows and tabs).
pub fn compact() -> bool {
    COMPACT.load(std::sync::atomic::Ordering::Relaxed)
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
    pub style: InterfaceStyle,
    pub title_bar: TitleBarStyle,
}

impl Default for AppearancePrefs {
    fn default() -> Self {
        Self {
            scheme: ColorScheme::Dark,
            dark_theme: presets::default_id(Appearance::Dark).into(),
            light_theme: presets::default_id(Appearance::Light).into(),
            toolbar: ToolbarStyle::default(),
            text_size: TextSize::default(),
            style: InterfaceStyle::default(),
            title_bar: TitleBarStyle::default(),
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
    applied: Option<(Appearance, String, InterfaceStyle)>,
}

impl Global for AppearanceState {}

fn file() -> Option<PathBuf> {
    prefs::path("appearance.json")
}

/// Loads the saved appearance and applies it. Called by `ferrum_ui::run`.
pub(crate) fn init(cx: &mut App) {
    init_at(file(), cx);
}

/// [`init`] with another preferences file (`None`: keep them in memory only,
/// as tests do).
pub(crate) fn init_at(file: Option<PathBuf>, cx: &mut App) {
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
    COMPACT.store(
        prefs.toolbar == ToolbarStyle::Compact,
        std::sync::atomic::Ordering::Relaxed,
    );
    let appearance = prefs.effective(cx.window_appearance());
    let key = (
        appearance,
        prefs.theme_id(appearance).to_owned(),
        prefs.style,
    );
    let unchanged = state.applied.as_ref() == Some(&key);
    cx.global_mut::<AppearanceState>().applied = Some(key);
    if !unchanged {
        let handled = hook.is_some_and(|hook| hook(&prefs, appearance, cx));
        if !handled {
            let mut theme = prefs.preset(appearance).theme();
            stylize(&mut theme, cx);
            GlobalTheme::update_theme(cx, Arc::new(theme));
        }
    }
    // A strip a shade off the backdrop, visible in every interface style.
    let fill = (prefs.title_bar == TitleBarStyle::Tinted).then(|| {
        let base = backdrop(cx).base;
        let text = crate::theme::ActiveTheme::theme(cx).colors().text;
        crate::surface::lighten(base, text, 0.06)
    });
    if let Ok(mut slot) = TITLE_BAR_FILL.lock() {
        *slot = fill;
    }
    cx.refresh_windows();
}

/// What a window paints behind its content in the current style: the
/// opaque theme background, an accent gradient, or a tint over the blur.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Backdrop {
    pub style: InterfaceStyle,
    /// The theme's opaque background.
    pub base: Hsla,
    pub accent: Hsla,
}

impl Global for Backdrop {}

impl Backdrop {
    /// The macOS window background for this style.
    pub fn window_background(&self) -> WindowBackgroundAppearance {
        match self.style {
            InterfaceStyle::Glass => WindowBackgroundAppearance::Blurred,
            _ => WindowBackgroundAppearance::Opaque,
        }
    }
}

/// The current [`Backdrop`].
pub fn backdrop(cx: &App) -> Backdrop {
    cx.try_global::<Backdrop>().copied().unwrap_or_else(|| {
        let colors = crate::theme::ActiveTheme::theme(cx).colors();
        Backdrop {
            style: InterfaceStyle::Solid,
            base: colors.background,
            accent: colors.text_accent,
        }
    })
}

/// Makes `theme` follow the interface style: in Gradient and Glass the
/// window background turns transparent (the window root paints the
/// [`Backdrop`]), bars float on it and panes become translucent. Menus and
/// dialogs (elevated surfaces) stay opaque. Hosts that install their own
/// themes (Ferrum Editor) call this before `GlobalTheme::update_theme`.
pub fn stylize(theme: &mut crate::theme::Theme, cx: &mut App) {
    let style = cx
        .try_global::<AppearanceState>()
        .map(|s| s.prefs.style)
        .unwrap_or_default();
    let c = &mut theme.styles.colors;
    cx.set_global(Backdrop {
        style,
        base: c.background,
        accent: c.text_accent,
    });
    if style == InterfaceStyle::Solid {
        return;
    }
    let pane = style.pane_alpha();
    let chrome = style.chrome_alpha();
    c.background = c.background.opacity(0.);
    for color in [
        &mut c.title_bar_background,
        &mut c.title_bar_inactive_background,
        &mut c.status_bar_background,
        &mut c.tab_bar_background,
        &mut c.tab_inactive_background,
        &mut c.toolbar_background,
    ] {
        *color = color.opacity(chrome);
    }
    for color in [
        &mut c.panel_background,
        &mut c.surface_background,
        &mut c.editor_background,
        &mut c.editor_gutter_background,
        &mut c.tab_active_background,
        &mut c.terminal_background,
    ] {
        *color = color.opacity(pane);
    }
}

/// The current interface style.
pub fn style(cx: &App) -> InterfaceStyle {
    cx.try_global::<AppearanceState>()
        .map(|s| s.prefs.style)
        .unwrap_or_default()
}

/// Applies the current choice again even if it did not change (a host
/// that reloaded its own theme calls this so the suite's choice wins back).
pub fn reapply(cx: &mut App) {
    if cx.has_global::<AppearanceState>() {
        cx.global_mut::<AppearanceState>().applied = None;
    }
    apply(cx);
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

    #[gpui::test]
    fn choosing_a_theme_recolours_the_app(cx: &mut gpui::TestAppContext) {
        use crate::theme::ActiveTheme;
        cx.update(|cx| {
            crate::init_for_tests(cx);
            init_at(None, cx);
            assert_eq!(cx.theme().name.as_ref(), "Ferrum Dark");
            update(cx, |p| p.choose_theme("plum-ink", Appearance::Dark));
            assert_eq!(cx.theme().name.as_ref(), "Plum Ink");
            update(cx, |p| p.scheme = ColorScheme::Light);
            assert_eq!(cx.theme().appearance, Appearance::Light);
            update(cx, |p| p.choose_theme("glacier", Appearance::Light));
            assert_eq!(cx.theme().name.as_ref(), "Glacier");
        });
    }

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

use gpui::{
    App, Bounds, Entity, KeyBinding, Pixels, Render, Size, TitlebarOptions, Window, WindowBounds,
    WindowOptions, actions, px, size,
};

use std::rc::Rc;

use gpui::AppContext as _;

use crate::app_info::{AppInfo, AppInfoGlobal};
use crate::assets::Assets;
use crate::menu;
use crate::root::Root;
use crate::settings::OpenSettings;
use crate::theme::{self, ThemeSettings};

actions!(ferrum, [Quit]);

/// Window and identity settings for a Ferrum tool.
pub struct AppConfig {
    pub title: &'static str,
    pub size: Size<Pixels>,
    pub min_size: Size<Pixels>,
    /// Onboarding, Settings pages and notifications; a bare one named after
    /// the title when not given.
    pub app: Option<AppInfo>,
    /// Hide the native title bar: the traffic lights float over the app,
    /// which draws its own title row (see [`crate::window_chrome`]).
    pub unified_titlebar: bool,
}

impl AppConfig {
    pub fn new(title: &'static str) -> Self {
        Self {
            title,
            size: size(px(1360.), px(860.)),
            min_size: size(px(900.), px(560.)),
            app: None,
            unified_titlebar: false,
        }
    }

    /// Describes the app for onboarding and Settings.
    pub fn app(mut self, info: AppInfo) -> Self {
        self.app = Some(info);
        self
    }

    /// The app draws its own title row (see [`crate::window_chrome`]).
    pub fn unified_titlebar(mut self) -> Self {
        self.unified_titlebar = true;
        self
    }

    pub fn size(mut self, width: f32, height: f32) -> Self {
        self.size = size(px(width), px(height));
        self
    }

    pub fn min_size(mut self, width: f32, height: f32) -> Self {
        self.min_size = size(px(width), px(height));
        self
    }
}

/// Boots GPUI with the Ferrum theme, fonts and icons, opens the main
/// window with the root view returned by `build`, and quits when it closes.
pub fn run<V: Render + 'static>(
    config: AppConfig,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) {
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            let info = config.app.unwrap_or_else(|| {
                let binary =
                    crate::prefs::current_binary().unwrap_or_else(|| config.title.to_lowercase());
                AppInfo::new(config.title, Box::leak(binary.into_boxed_str()))
            });
            cx.set_global(AppInfoGlobal(Rc::new(info)));
            if config.unified_titlebar {
                cx.set_global(crate::root::UnifiedTitlebar);
            }
            init(config.title, cx);

            let bounds = Bounds::centered(None, config.size, cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(config.min_size),
                titlebar: Some(TitlebarOptions {
                    title: Some(config.title.into()),
                    appears_transparent: config.unified_titlebar,
                    traffic_light_position: config
                        .unified_titlebar
                        .then(crate::window_chrome::traffic_light_position),
                }),
                app_id: Some(config.title.to_lowercase()),
                ..Default::default()
            };
            let opened = cx.open_window(options, |window, cx| {
                let app = build(window, cx);
                cx.new(|cx| Root::new(app.into(), window, cx))
            });
            // After `build`, so recorded shortcuts win over the app's own.
            crate::keymap::apply_saved(cx);
            crate::appearance::watch(cx);
            // Development aid: open Settings at start (`FERRUM_OPEN_SETTINGS=terminal`).
            if let Ok(page) = std::env::var("FERRUM_OPEN_SETTINGS") {
                crate::settings::open_settings(
                    Some(&page).filter(|p| !p.is_empty()).map(|p| p.as_str()),
                    cx,
                );
            }
            if let Err(error) = opened {
                eprintln!("{}: failed to open window: {error:#}", config.title);
                cx.quit();
                return;
            }
            if crate::dock::hidden_by_preference() {
                crate::dock::set_dock_icon_visible(false);
            }
            cx.activate(true);
        });
}

fn init(title: &'static str, cx: &mut App) {
    if let Err(error) = Assets.load_fonts(cx) {
        eprintln!("{title}: failed to load bundled fonts: {error:#}");
    }
    theme::init(ThemeSettings::default(), cx);
    crate::appearance::init(cx);

    crate::text_input::bind_keys(cx);
    crate::project_bar::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("escape", menu::Cancel, None),
        KeyBinding::new("enter", menu::Confirm, Some("menu")),
        KeyBinding::new("enter", menu::Confirm, Some("Onboarding")),
        KeyBinding::new("up", menu::SelectPrevious, Some("menu")),
        KeyBinding::new("down", menu::SelectNext, Some("menu")),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &OpenSettings, cx| crate::settings::open_settings(None, cx));
    crate::menus::register(cx);
    let info = crate::app_info::app_info(cx);
    cx.set_menus(crate::menus::standard(&info));
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

/// Theme and fonts for headless GPUI tests (system font names, since tests
/// do not load the bundled fonts).
pub fn init_for_tests(cx: &mut App) {
    theme::init(ThemeSettings::with_fonts("Helvetica", "Menlo"), cx);
}

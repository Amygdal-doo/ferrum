use gpui::{
    AnyWindowHandle, App, Bounds, Entity, Global, KeyBinding, Pixels, Render, Size,
    TitlebarOptions, Window, WindowBounds, WindowOptions, actions, point, px, size,
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
    /// Without a Dock icon, carry the app's menus in a menu bar extra (see
    /// [`crate::dock`]). Off for apps with a menu bar extra of their own.
    pub status_item: bool,
    /// Called with the URLs (`file://` for files) the platform asks the
    /// running app to open: Finder's Open With, a drop on the Dock icon.
    pub open_urls: Option<Box<dyn FnMut(Vec<String>)>>,
}

/// How the main window was opened, for [`open_app_window`].
#[derive(Clone, Copy)]
struct MainWindowStyle {
    title: &'static str,
    size: Size<Pixels>,
    min_size: Size<Pixels>,
    unified_titlebar: bool,
}

impl Global for MainWindowStyle {}

impl AppConfig {
    pub fn new(title: &'static str) -> Self {
        Self {
            title,
            size: size(px(1360.), px(860.)),
            min_size: size(px(900.), px(560.)),
            app: None,
            unified_titlebar: false,
            status_item: true,
            open_urls: None,
        }
    }

    /// Handles URLs the platform asks the running app to open (see
    /// [`AppConfig::open_urls`]).
    pub fn on_open_urls(mut self, handler: impl FnMut(Vec<String>) + 'static) -> Self {
        self.open_urls = Some(Box::new(handler));
        self
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

    /// The app has its own menu bar extra: none for its menus when it runs
    /// without a Dock icon.
    pub fn without_status_item(mut self) -> Self {
        self.status_item = false;
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
    mut config: AppConfig,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) {
    let hidden_from_dock = crate::dock::hidden_by_preference();
    let mut application = gpui_platform::application().with_assets(Assets);
    if cfg!(target_os = "macos") && hidden_from_dock {
        // `Accessory` from launch, so no Dock icon shows up even briefly.
        // (Elsewhere `Headless` would mean no display at all.)
        application = application.with_windowing(gpui::WindowingRequest::Headless);
    }
    if let Some(open_urls) = config.open_urls.take() {
        application.on_open_urls(open_urls);
    }
    application.run(move |cx: &mut App| {
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
        crate::dock::use_status_item(config.status_item.then(|| info_name(cx)).as_deref());

        cx.set_global(MainWindowStyle {
            title: config.title,
            size: config.size,
            min_size: config.min_size,
            unified_titlebar: config.unified_titlebar,
        });
        let opened = open_app_window(cx, build);
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
        if hidden_from_dock {
            crate::dock::set_dock_icon_visible(false);
        } else if crate::dock::in_menu_bar_by_preference() {
            crate::dock::set_menu_bar_icon_visible(true);
        }
        cx.activate(true);
    });
}

/// Opens another window like the main one (size, title bar, root view
/// chrome) with the view `build` returns: apps with several windows (one
/// per project). Cascades from the active window. Only after [`run`].
pub fn open_app_window<V: Render + 'static>(
    cx: &mut App,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V>,
) -> anyhow::Result<AnyWindowHandle> {
    let style = *cx
        .try_global::<MainWindowStyle>()
        .ok_or_else(|| anyhow::anyhow!("open_app_window before run"))?;
    let mut bounds = Bounds::centered(None, style.size, cx);
    if let Some(active) = cx.active_window()
        && let Ok(active_bounds) = active.update(cx, |_, window, _| window.bounds())
    {
        bounds.origin = active_bounds.origin + point(px(28.), px(28.));
    }
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        window_min_size: Some(style.min_size),
        titlebar: Some(TitlebarOptions {
            title: Some(style.title.into()),
            appears_transparent: style.unified_titlebar,
            traffic_light_position: style
                .unified_titlebar
                .then(crate::window_chrome::traffic_light_position),
        }),
        app_id: Some(style.title.to_lowercase()),
        ..Default::default()
    };
    let handle = cx.open_window(options, |window, cx| {
        let app = build(window, cx);
        cx.new(|cx| Root::new(app.into(), window, cx))
    })?;
    Ok(handle.into())
}

fn info_name(cx: &App) -> String {
    crate::app_info::app_info(cx).name.to_string()
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
    cx.on_action(|_: &OpenSettings, cx| {
        // Also chosen from the menu bar extra while the app is inactive.
        cx.activate(true);
        crate::settings::open_settings(None, cx);
    });
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

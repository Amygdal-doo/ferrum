use assets::Assets;
use gpui::{
    App, Bounds, Entity, Font, KeyBinding, Menu, MenuItem, Pixels, Render, Size, TitlebarOptions,
    Window, WindowBounds, WindowOptions, actions, font, px, size,
};
use theme::{LoadThemes, ThemeSettingsProvider, UiDensity};

actions!(ferrum, [Quit]);

/// Window and identity settings for a Ferrum tool.
pub struct AppConfig {
    pub title: &'static str,
    pub size: Size<Pixels>,
    pub min_size: Size<Pixels>,
}

impl AppConfig {
    pub fn new(title: &'static str) -> Self {
        Self {
            title,
            size: size(px(1360.), px(860.)),
            min_size: size(px(900.), px(560.)),
        }
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

struct FerrumThemeSettings {
    ui_font: Font,
    buffer_font: Font,
}

impl ThemeSettingsProvider for FerrumThemeSettings {
    fn ui_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.ui_font
    }

    fn buffer_font<'a>(&'a self, _: &'a App) -> &'a Font {
        &self.buffer_font
    }

    fn ui_font_size(&self, _: &App) -> Pixels {
        px(14.)
    }

    fn buffer_font_size(&self, _: &App) -> Pixels {
        px(13.)
    }

    fn ui_density(&self, _: &App) -> UiDensity {
        UiDensity::Default
    }
}

/// Boots GPUI with the FerrumIDE theme, fonts and icons, opens the main
/// window with the root view returned by `build`, and quits when it closes.
pub fn run<V: Render + 'static>(
    config: AppConfig,
    build: impl FnOnce(&mut Window, &mut App) -> Entity<V> + 'static,
) {
    gpui_platform::application()
        .with_assets(Assets)
        .run(move |cx: &mut App| {
            init(config.title, cx);

            let bounds = Bounds::centered(None, config.size, cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(config.min_size),
                titlebar: Some(TitlebarOptions {
                    title: Some(config.title.into()),
                    ..Default::default()
                }),
                app_id: Some(config.title.to_lowercase()),
                ..Default::default()
            };
            if let Err(error) = cx.open_window(options, |window, cx| build(window, cx)) {
                eprintln!("{}: failed to open window: {error:#}", config.title);
                cx.quit();
                return;
            }
            cx.activate(true);
        });
}

fn init(title: &'static str, cx: &mut App) {
    if let Err(error) = Assets.load_fonts(cx) {
        eprintln!("{title}: failed to load bundled fonts: {error:#}");
    }
    theme::set_theme_settings_provider(
        Box::new(FerrumThemeSettings {
            ui_font: font(".ZedSans"),
            buffer_font: font(".ZedMono"),
        }),
        cx,
    );
    theme::init(LoadThemes::JustBase, cx);

    crate::text_input::bind_keys(cx);
    cx.bind_keys([
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("escape", menu::Cancel, None),
        KeyBinding::new("enter", menu::Confirm, Some("menu")),
        KeyBinding::new("up", menu::SelectPrevious, Some("menu")),
        KeyBinding::new("down", menu::SelectNext, Some("menu")),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.set_menus([Menu::new(title).items([MenuItem::action("Quit", Quit)])]);
    cx.on_window_closed(|cx, _| {
        if cx.windows().is_empty() {
            cx.quit();
        }
    })
    .detach();
}

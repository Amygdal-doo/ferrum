//! FerrumKit desktop entry point. Everything runs locally; the app makes no
//! network requests and stores only input history and saved inputs (in
//! `~/.ferrum/kit/`).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod app_info;
mod history;
mod palette;
mod search;
#[cfg(test)]
mod tests_render;
mod tools;
mod widgets;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;

fn main() {
    let config = AppConfig::new("FerrumKit")
        .size(1180., 760.)
        .min_size(820., 520.)
        .app(app_info::app_info())
        .unified_titlebar();
    ferrum_ui::run(config, |window, cx| {
        ferrum_account_ui::init(ferrum_account_ui::AccountApp::Kit, cx);
        app::bind_keys(cx);
        cx.new(|cx| app::FerrumKit::new(window, cx))
    });
}

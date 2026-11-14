//! FerrumEnv desktop entry point: a password-locked vault for project
//! environment variables. Nothing is sent over the network; the only file
//! written is the encrypted vault (plus `.env` files the user exports).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod app_info;
mod dialogs;
mod lock_screen;
mod secrets;
mod table;
mod unlocked;
mod widgets;

#[cfg(test)]
mod tests;

use ferrum_ui::AppConfig;
use ferrum_ui::gpui::AppContext as _;

fn main() {
    let config = AppConfig::new("FerrumEnv")
        .size(1180., 760.)
        .min_size(860., 540.)
        .unified_titlebar()
        .app(app_info::app_info());
    ferrum_ui::run(config, |window, cx| {
        app::bind_keys(cx);
        let config = app::EnvConfig::from_home();
        cx.new(|cx| app::EnvApp::new(config, window, cx))
    });
}

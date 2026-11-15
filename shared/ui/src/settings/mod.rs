//! The Settings window every Ferrum app shares (⌘,): Interface, Themes,
//! Shortcuts, System (shell command, `ferrum` command, Dock icon),
//! Integrations and Notifications, plus pages the app adds
//! through [`crate::AppInfo::page`] (FerrumTerm's Terminal and Remote Hosts,
//! …). Pages are drawn with [`widgets`].

pub(crate) mod about_page;
mod integrations_page;
pub(crate) mod interface_page;
mod notifications_page;
mod shortcuts_page;
pub(crate) mod system_page;
pub(crate) mod themes_page;
pub mod widgets;
mod window;

use std::rc::Rc;

use gpui::{AnyElement, App, Hsla, Window};

use crate::components::IconName;

pub use window::{SettingsWindow, open_settings};

gpui::actions!(
    ferrum,
    [
        /// Open the Settings window.
        OpenSettings,
        /// Show the first-launch walkthrough again.
        ShowOnboarding
    ]
);

/// One page of the Settings window.
pub trait SettingsPage {
    /// Stable id, used for navigation (`open_settings(Some("terminal"))`).
    fn id(&self) -> &'static str;
    fn title(&self) -> &'static str;
    fn icon(&self) -> IconName;
    /// Colour of the sidebar icon tile.
    fn tint(&self) -> Hsla;
    /// Extra words the sidebar search matches.
    fn keywords(&self) -> &'static [&'static str] {
        &[]
    }
    /// The page body. Changing a setting should end with
    /// `cx.refresh_windows()` so this window redraws.
    fn render(&self, window: &mut Window, cx: &mut App) -> AnyElement;
}

/// The shared pages, in sidebar order around the app's own pages.
type Pages = Vec<Rc<dyn SettingsPage>>;

pub(crate) fn shared_pages() -> (Pages, Pages) {
    (
        vec![
            Rc::new(interface_page::InterfacePage),
            Rc::new(themes_page::ThemesPage),
        ],
        vec![
            Rc::new(shortcuts_page::ShortcutsPage::default()),
            Rc::new(system_page::SystemPage::default()),
            Rc::new(integrations_page::IntegrationsPage),
            Rc::new(notifications_page::NotificationsPage),
            Rc::new(about_page::AboutPage),
        ],
    )
}

pub(crate) fn tint(rgb: u32) -> Hsla {
    gpui::rgb(rgb).into()
}

//! The Settings window every Ferrum app shares (⌘,): Interface, Themes,
//! Shortcuts, System (shell command, `ferrum` command, Dock icon),
//! Integrations and Notifications, plus pages the app adds
//! through [`crate::AppInfo::page`] (FerrumTerm's Terminal and Remote Hosts,
//! …) and shared pages from other crates ([`add_shared_page`]: Account,
//! from `ferrum-account-ui`). Pages are drawn with [`widgets`].

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

use gpui::{AnyElement, App, Global, Hsla, Window};

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

/// Pages added with [`add_shared_page`].
#[derive(Default)]
struct ExtraPages(Pages);

impl Global for ExtraPages {}

/// Adds a page every Settings window of this app shows, after Interface
/// and Themes and before the app's own pages; for shared pages that live
/// outside this crate (Settings › Account). A page whose id is already
/// there is not added twice.
pub fn add_shared_page(page: Rc<dyn SettingsPage>, cx: &mut App) {
    let pages = &mut cx.default_global::<ExtraPages>().0;
    if pages.iter().all(|p| p.id() != page.id()) {
        pages.push(page);
    }
}

/// The pages added with [`add_shared_page`], in the order added.
pub(crate) fn extra_pages(cx: &App) -> Pages {
    cx.try_global::<ExtraPages>()
        .map(|pages| pages.0.clone())
        .unwrap_or_default()
}

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

/// Every page of this app's Settings window, in sidebar order.
pub(crate) fn all_pages(cx: &App) -> Pages {
    let (head, tail) = shared_pages();
    let mut pages = head;
    pages.extend(extra_pages(cx));
    pages.extend(crate::app_info(cx).pages.iter().cloned());
    pages.extend(tail);
    pages
}

pub(crate) fn tint(rgb: u32) -> Hsla {
    gpui::rgb(rgb).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Page(&'static str);

    impl SettingsPage for Page {
        fn id(&self) -> &'static str {
            self.0
        }
        fn title(&self) -> &'static str {
            self.0
        }
        fn icon(&self) -> IconName {
            IconName::Person
        }
        fn tint(&self) -> Hsla {
            tint(0)
        }
        fn render(&self, _: &mut Window, _: &mut App) -> AnyElement {
            gpui::IntoElement::into_any_element(gpui::div())
        }
    }

    #[gpui::test]
    fn shared_pages_are_added_once_in_order(cx: &mut gpui::TestAppContext) {
        cx.update(|cx| {
            assert!(extra_pages(cx).is_empty());
            add_shared_page(Rc::new(Page("account")), cx);
            add_shared_page(Rc::new(Page("sync")), cx);
            add_shared_page(Rc::new(Page("account")), cx);
            let ids: Vec<_> = extra_pages(cx).iter().map(|p| p.id()).collect();
            assert_eq!(ids, ["account", "sync"]);
        });
    }
}

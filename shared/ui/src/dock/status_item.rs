//! The app's menus in a menu bar extra, for an app hidden from the Dock.
//!
//! macOS gives the menu bar only to apps with the `Regular` activation
//! policy, which always have a Dock icon: an `Accessory` app keeps the
//! previous app's menu bar even while its own window is key (checked with
//! `NSWorkspace.menuBarOwningApplication`). So an app without a Dock icon
//! shows its menu bar folded into a status item instead: one submenu per
//! menu of the main menu (`NSApp.mainMenu`). The copies keep GPUI's tags and
//! validation, so every item behaves as in the real menu bar; they are
//! rebuilt whenever the app replaces its menus ([`refresh`]).

use std::cell::RefCell;

use objc2::MainThreadMarker;
use objc2::rc::Retained;
use objc2_app_kit::{
    NSApplication, NSImage, NSMenu, NSMenuItem, NSStatusBar, NSStatusItem,
    NSVariableStatusItemLength,
};
use objc2_foundation::{NSCopying, NSSize, NSString};

struct Installed {
    item: Retained<NSStatusItem>,
    app_name: String,
}

thread_local! {
    static INSTALLED: RefCell<Option<Installed>> = const { RefCell::new(None) };
}

/// Shows the status item (once) carrying the app's menus.
pub fn show(app_name: &str, mtm: MainThreadMarker) {
    INSTALLED.with_borrow_mut(|installed| {
        if installed.is_some() {
            return;
        }
        let item = NSStatusBar::systemStatusBar().statusItemWithLength(NSVariableStatusItemLength);
        if let Some(button) = item.button(mtm) {
            let name = NSString::from_str(app_name);
            match icon(mtm) {
                Some(image) => button.setImage(Some(&image)),
                None => button.setTitle(&name),
            }
            button.setToolTip(Some(&name));
        }
        item.setMenu(Some(&menu(app_name, mtm)));
        *installed = Some(Installed {
            item,
            app_name: app_name.to_owned(),
        });
    });
}

/// Rebuilds the status item's menu from the current main menu.
pub fn refresh(mtm: MainThreadMarker) {
    INSTALLED.with_borrow(|installed| {
        if let Some(Installed { item, app_name }) = installed {
            item.setMenu(Some(&menu(app_name, mtm)));
        }
    });
}

/// Removes the status item, if shown.
pub fn hide() {
    INSTALLED.with_borrow_mut(|installed| {
        if let Some(Installed { item, .. }) = installed.take() {
            NSStatusBar::systemStatusBar().removeStatusItem(&item);
        }
    });
}

/// The app icon at menu bar size.
fn icon(mtm: MainThreadMarker) -> Option<Retained<NSImage>> {
    let image = NSApplication::sharedApplication(mtm)
        .applicationIconImage()?
        .copy();
    image.setSize(NSSize::new(18.0, 18.0));
    Some(image)
}

/// One submenu per menu of the main menu, the first (the app menu) named
/// after the app.
fn menu(app_name: &str, mtm: MainThreadMarker) -> Retained<NSMenu> {
    let menu = NSMenu::new(mtm);
    let Some(main_menu) = NSApplication::sharedApplication(mtm).mainMenu() else {
        return menu;
    };
    let entries = main_menu.itemArray();
    let titles: Vec<String> = entries.iter().map(|e| e.title().to_string()).collect();
    for (entry, title) in entries
        .iter()
        .zip(super::status_menu_titles(app_name, &titles))
    {
        let Some(submenu) = entry.submenu() else {
            continue;
        };
        let item = NSMenuItem::new(mtm);
        item.setTitle(&NSString::from_str(&title));
        item.setSubmenu(Some(&submenu.copy()));
        menu.addItem(&item);
    }
    menu
}

//! FerrumSystem's menu bar: the shared one with File › New Diagram (⌘N),
//! Open… (⌘O) and Open Demo ›, View toggles for the sidebar (⌘B) and the
//! right panel (⌥⌘B) plus zoom, and Help › Open Demo.

use ferrum_ui::gpui::{Menu, MenuItem, OsAction};
use ferrum_ui::menus::{
    CloseWindow, LargerText, SmallerText, ToggleFullScreen, app_menu, edit_menu, help_menu,
    window_menu,
};
use ferrum_ui::{AppInfo, ToggleProjectSwitcher};
use ferrumsystem_core::demos;

use crate::app::{
    NewDiagram, OpenDemo, OpenProject, Redo, TogglePanel, ToggleSidebar, Undo, ZoomFit, ZoomIn,
    ZoomOut,
};

#[must_use]
pub fn menus(info: &AppInfo) -> Vec<Menu> {
    let demo_items: Vec<MenuItem> = demos::all()
        .into_iter()
        .enumerate()
        .map(|(index, demo)| MenuItem::action(demo.name, OpenDemo { index }))
        .collect();
    // Edit › Undo / Redo the diagram's edits, above Cut, Copy, Paste.
    let mut edit = edit_menu();
    edit.items.splice(
        0..0,
        [
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
        ],
    );
    let mut help = help_menu(info);
    help.items
        .insert(1, MenuItem::action("Open Demo", OpenDemo { index: 0 }));
    vec![
        app_menu(info),
        Menu::new("File").items([
            MenuItem::action("New Diagram", NewDiagram),
            MenuItem::action("Open…", OpenProject),
            MenuItem::action("Switch Project…", ToggleProjectSwitcher),
            MenuItem::separator(),
            MenuItem::submenu(Menu::new("Open Demo").items(demo_items)),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        edit,
        Menu::new("View").items([
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::action("Toggle Panel", TogglePanel),
            MenuItem::separator(),
            MenuItem::action("Zoom In", ZoomIn),
            MenuItem::action("Zoom Out", ZoomOut),
            MenuItem::action("Zoom to Fit", ZoomFit),
            MenuItem::separator(),
            MenuItem::action("Larger Text", LargerText),
            MenuItem::action("Smaller Text", SmallerText),
            MenuItem::separator(),
            MenuItem::action("Enter Full Screen", ToggleFullScreen),
        ]),
        window_menu(),
        help,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn labels(menu: &Menu) -> Vec<String> {
        menu.items
            .iter()
            .map(|item| match item {
                MenuItem::Separator => "-".to_string(),
                MenuItem::Submenu(menu) => format!("{} ›", menu.name),
                MenuItem::SystemMenu(menu) => format!("{} ›", menu.name),
                MenuItem::Action { name, .. } => name.to_string(),
            })
            .collect()
    }

    fn menu<'a>(menus: &'a [Menu], name: &str) -> &'a Menu {
        menus
            .iter()
            .find(|m| m.name.as_ref() == name)
            .expect("menu")
    }

    #[test]
    fn the_demo_and_both_panes_are_in_the_menu_bar() {
        let menus = menus(&crate::app_info::app_info());
        let file = menu(&menus, "File");
        assert!(labels(file).contains(&"Open Demo ›".to_string()));
        let Some(MenuItem::Submenu(demos)) = file.items.get(4) else {
            panic!("File › Open Demo is a submenu");
        };
        assert_eq!(demos.items.len(), demos::all().len());
        let help = labels(menu(&menus, "Help"));
        assert_eq!(help[1], "Open Demo");
        let view = labels(menu(&menus, "View"));
        assert_eq!(view[..2], ["Toggle Sidebar", "Toggle Panel"]);
        assert_eq!(labels(menu(&menus, "Edit"))[..2], ["Undo", "Redo"]);
    }
}

//! FerrumEngineer's menu bar: the shared one with File › Open Folder… (⌘O),
//! New Task and New Discussion, and View toggles for the sidebar and the
//! Files, Git, Term and Editor panes, and moves of the active pane.

use ferrum_ui::gpui::{Menu, MenuItem};
use ferrum_ui::menus::{
    CloseWindow, LargerText, SmallerText, ToggleFullScreen, app_menu, edit_menu, help_menu,
    window_menu,
};
use ferrum_ui::{AppInfo, ToggleProjectSwitcher};

use crate::app::{
    MovePaneLeft, MovePaneRight, NewDiscussion, NewTask, OpenFolder, ToggleEditor, ToggleFiles,
    ToggleGit, ToggleSidebar, ToggleTerm,
};

#[must_use]
pub fn menus(info: &AppInfo) -> Vec<Menu> {
    vec![
        app_menu(info),
        Menu::new("File").items([
            MenuItem::action("Open Folder…", OpenFolder),
            MenuItem::action("Switch Project…", ToggleProjectSwitcher),
            MenuItem::separator(),
            MenuItem::action("New Task", NewTask),
            MenuItem::action("New Discussion", NewDiscussion),
            MenuItem::separator(),
            MenuItem::action("Close Window", CloseWindow),
        ]),
        edit_menu(),
        Menu::new("View").items([
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::action("Toggle Files", ToggleFiles),
            MenuItem::action("Toggle Git", ToggleGit),
            MenuItem::action("Toggle Terminal", ToggleTerm),
            MenuItem::action("Toggle Editor", ToggleEditor),
            MenuItem::action("Move Pane Left", MovePaneLeft),
            MenuItem::action("Move Pane Right", MovePaneRight),
            MenuItem::separator(),
            MenuItem::action("Larger Text", LargerText),
            MenuItem::action("Smaller Text", SmallerText),
            MenuItem::separator(),
            MenuItem::action("Enter Full Screen", ToggleFullScreen),
        ]),
        window_menu(),
        help_menu(info),
    ]
}

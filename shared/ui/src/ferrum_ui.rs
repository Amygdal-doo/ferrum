//! Shared GPUI foundation for the Ferrum desktop tools (FerrumAPI, FerrumGIT,
//! FerrumData, FerrumTerm, ...): the app shell, Ferrum's theme, its widget
//! set and bundled icons/fonts, so every tool looks and behaves alike.
//! Built directly on `gpui` (Apache-2.0).

pub mod agent_logo;
mod app;
mod app_info;
pub mod appearance;
pub mod assets;
mod chrome;
pub mod components;
pub mod dock;
pub mod integrations;
pub mod keymap;
pub mod mac_notifications;
pub mod menu;
pub mod menus;
mod modal;
mod notice;
pub mod notify;
pub mod onboarding;
pub mod prefs;
pub mod project;
mod project_bar;
mod root;
pub mod settings;
mod splitter;
pub mod surface;
mod text_input;
pub mod theme;
pub mod window_chrome;

pub use agent_logo::{AgentLogo, agent_logo, terminal_tile};
pub use app::{AppConfig, Quit, init_for_tests, run};
pub use app_info::{
    AppInfo, KeyHint, Migration, NotificationEvent, Permission, PermissionKind, app_info,
};
pub use chrome::{badge, empty_state, kbd, section_header, status_bar, toolbar};
pub use modal::Modal;
pub use notice::{Notice, NoticeTone, render_notice};
pub use project_bar::{ProjectBar, ProjectBarEvent, ToggleProjectSwitcher};
pub use settings::{OpenSettings, ShowOnboarding, open_settings};
pub use splitter::{DraggedSplitter, splitter};
pub use surface::{PANE_GAP, backdrop, gap_splitter, pane_frame, pane_header};
pub use text_input::{TextInput, TextInputEvent};
pub use window_chrome::{
    TITLE_ROW_HEIGHT, active_fill, command_button, header_tab, hover_fill, pane_card, sidebar_item,
    sidebar_label, sidebar_toggle, title_button, title_row,
};

pub use gpui;

pub mod prelude {
    pub use crate::components::prelude::*;
    pub use crate::components::{
        ContextMenu, Divider, DropdownMenu, ListItem, PopoverMenu, Tab, TabBar, ToggleState,
        Tooltip,
    };
    pub use crate::{
        AppConfig, DraggedSplitter, Modal, Notice, NoticeTone, ProjectBar, ProjectBarEvent,
        TextInput, TextInputEvent, ToggleProjectSwitcher, badge, empty_state, kbd, render_notice,
        section_header, splitter, status_bar, toolbar,
    };
    pub use gpui::{
        AnyElement, App, AppContext as _, AsyncApp, ClickEvent, Context, DragMoveEvent, Entity,
        EventEmitter, FocusHandle, Focusable, IntoElement, KeyBinding, ParentElement, Render,
        SharedString, Subscription, Task, WeakEntity, Window, actions, div, px, rems, uniform_list,
    };
}

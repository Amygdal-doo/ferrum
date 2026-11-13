//! Shared GPUI foundation for the Ferrum desktop tools (FerrumAPI, FerrumGIT,
//! FerrumData, FerrumTerm, ...): the app shell, Ferrum's theme, its widget
//! set and bundled icons/fonts, so every tool looks and behaves alike.
//! Built directly on `gpui` (Apache-2.0).

mod app;
pub mod assets;
mod chrome;
pub mod components;
pub mod dock;
pub mod menu;
mod modal;
mod notice;
pub mod project;
mod project_bar;
mod splitter;
mod text_input;
pub mod theme;

pub use app::{AppConfig, Quit, init_for_tests, run};
pub use chrome::{badge, empty_state, kbd, section_header, status_bar, toolbar};
pub use modal::Modal;
pub use notice::{Notice, NoticeTone, render_notice};
pub use project_bar::{ProjectBar, ProjectBarEvent, ToggleProjectSwitcher};
pub use splitter::{DraggedSplitter, splitter};
pub use text_input::{TextInput, TextInputEvent};

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

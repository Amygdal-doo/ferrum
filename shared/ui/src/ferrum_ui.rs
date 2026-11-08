//! Shared GPUI foundation for the Ferrum desktop tools (FerrumAPI, FerrumGIT,
//! FerrumData, FerrumTerm). Builds on the `gpui`, `ui` and `theme` crates from
//! the FerrumIDE fork so every tool looks and behaves like the IDE.

mod app;
mod chrome;
mod modal;
mod notice;
mod splitter;
mod text_input;

pub use app::{AppConfig, Quit, run};
pub use chrome::{badge, empty_state, kbd, section_header, status_bar, toolbar};
pub use modal::Modal;
pub use notice::{Notice, NoticeTone, render_notice};
pub use splitter::{DraggedSplitter, splitter};
pub use text_input::{TextInput, TextInputEvent};

pub use gpui;
pub use theme;
pub use ui;

pub mod prelude {
    pub use crate::{
        AppConfig, DraggedSplitter, Modal, Notice, NoticeTone, TextInput, TextInputEvent, badge,
        empty_state, kbd, render_notice, section_header, splitter, status_bar, toolbar,
    };
    pub use gpui::{
        AnyElement, App, AppContext as _, AsyncApp, ClickEvent, Context, DragMoveEvent, Entity,
        EventEmitter, FocusHandle, Focusable, IntoElement, KeyBinding, ParentElement, Render,
        SharedString, Subscription, Task, WeakEntity, Window, actions, div, px, rems,
        uniform_list,
    };
    pub use ui::prelude::*;
    pub use ui::{
        ContextMenu, Divider, DropdownMenu, ListItem, PopoverMenu, Tab, TabBar, ToggleState,
        Tooltip,
    };
}

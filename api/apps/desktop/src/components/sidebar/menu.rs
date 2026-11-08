//! Row context menus and the inline rename field.

use crate::app::FerrumApi;
use crate::components::fields::{FieldEvent, FieldSpec};
use crate::components::{MenuHandler, action_menu};
use ferrum_ui::TextInput;
use gpui::{AnyElement, Context, ElementId, Entity, SharedString, Window, prelude::*};
use ui::{IconButton, IconName, IconSize, PopoverMenu, right_click_menu};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Open,
    AddRequest,
    AddFolder,
    Settings,
    Rename,
    Duplicate,
    Export,
    Delete,
}

impl MenuAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open in tab",
            Self::AddRequest => "Add request",
            Self::AddFolder => "Add folder",
            Self::Settings => "Auth & variables…",
            Self::Rename => "Rename",
            Self::Duplicate => "Duplicate",
            Self::Export => "Export…",
            Self::Delete => "Delete…",
        }
    }
}

/// "⋯" button that opens `entries` as a popover menu.
pub fn more_button(
    id: impl Into<ElementId>,
    entries: Vec<(SharedString, MenuHandler)>,
    cx: &mut Context<FerrumApi>,
) -> impl IntoElement {
    let weak = cx.weak_entity();
    PopoverMenu::new(id)
        .trigger(IconButton::new("more", IconName::Ellipsis).icon_size(IconSize::Small))
        .menu(move |window, cx| Some(action_menu(window, cx, weak.clone(), entries.clone())))
}

/// Wraps `row` so right-clicking it opens `entries`.
pub fn with_context_menu(
    id: impl Into<ElementId>,
    row: AnyElement,
    entries: Vec<(SharedString, MenuHandler)>,
    cx: &mut Context<FerrumApi>,
) -> AnyElement {
    let weak = cx.weak_entity();
    right_click_menu(id)
        .trigger(move |_, _, _| row)
        .menu(move |window, cx| action_menu(window, cx, weak.clone(), entries.clone()))
        .into_any_element()
}

impl FerrumApi {
    /// Inline rename field; commits on Enter / blur, cancels on Escape.
    pub fn rename_field(
        &mut self,
        uid: Uuid,
        value: &str,
        commit: impl Fn(&mut FerrumApi, String, &mut Context<FerrumApi>) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<TextInput> {
        let spec = FieldSpec::new(format!("rename:{uid}"))
            .initial(value)
            .autofocus();
        self.field(
            spec,
            move |this, event, _, cx| {
                if matches!(event, FieldEvent::Submit | FieldEvent::Blurred)
                    && this.state.renaming == Some(uid)
                {
                    this.state.renaming = None;
                    let name = this
                        .fields
                        .text(&format!("rename:{uid}"), cx)
                        .unwrap_or_default();
                    commit(this, name, cx);
                }
            },
            window,
            cx,
        )
    }
}

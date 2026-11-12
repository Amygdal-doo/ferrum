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
    Run,
    MockServer,
    AddRequest,
    AddFolder,
    Settings,
    Rename,
    Duplicate,
    Export,
    MoveToProject,
    MakeShared,
    Delete,
}

impl MenuAction {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Open => "Open in tab",
            Self::Run => "Run…",
            Self::MockServer => "Mock server…",
            Self::AddRequest => "Add request…",
            Self::AddFolder => "Add folder",
            Self::Settings => "Auth & variables…",
            Self::Rename => "Rename",
            Self::Duplicate => "Duplicate",
            Self::Export => "Export…",
            Self::MoveToProject => "Move to current project",
            Self::MakeShared => "Make shared (all projects)",
            Self::Delete => "Delete…",
        }
    }
}

/// A collection's menu: the container actions plus mock server, settings
/// and moving between the current project and shared.
#[must_use]
pub fn collection_actions(
    container: &[MenuAction],
    project: Option<&str>,
    owner: Option<&str>,
) -> Vec<MenuAction> {
    let mut actions = container.to_vec();
    actions.insert(3, MenuAction::Settings);
    actions.insert(1, MenuAction::MockServer);
    if project.is_some() && owner != project {
        actions.insert(actions.len() - 1, MenuAction::MoveToProject);
    }
    if owner.is_some() {
        actions.insert(actions.len() - 1, MenuAction::MakeShared);
    }
    actions
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

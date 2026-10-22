//! Context menu (fixed at the pointer) and inline rename input.

use crate::state::AppState;
use dioxus::prelude::*;
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

/// Renders a menu at `pos` with a click-away backdrop.
#[component]
pub fn ContextMenu(
    pos: (f64, f64),
    actions: Vec<MenuAction>,
    on_pick: EventHandler<MenuAction>,
    on_close: EventHandler<()>,
) -> Element {
    let (x, y) = pos;
    rsx! {
        div { class: "fixed inset-0 z-30", onclick: move |_| on_close.call(()),
            oncontextmenu: move |e| { e.prevent_default(); on_close.call(()); } }
        div {
            class: "fixed z-40 min-w-44 rounded-md border bg-popover py-1 text-xs text-popover-foreground shadow-lg",
            style: "left: {x}px; top: {y}px",
            role: "menu",
            for a in actions {
                {
                    let danger = if a == MenuAction::Delete { "text-destructive" } else { "" };
                    rsx! {
                        button {
                            key: "{a.label()}",
                            class: "block w-full px-3 py-1.5 text-left hover:bg-accent {danger}",
                            role: "menuitem",
                            onclick: move |e| { e.stop_propagation(); on_close.call(()); on_pick.call(a); },
                            "{a.label()}"
                        }
                    }
                }
            }
        }
    }
}

/// Inline rename field; commits on Enter / blur, cancels on Escape.
#[component]
pub fn RenameInput(id: Uuid, value: String, on_commit: EventHandler<String>) -> Element {
    let mut state = use_context::<AppState>();
    rsx! {
        input {
            class: "h-6 min-w-0 flex-1 rounded border bg-background px-1 text-sm select-text",
            onmounted: |e: MountedEvent| async move { let _ = e.data().set_focus(true).await; },
            "aria-label": "New name",
            initial_value: "{value}",
            onclick: move |e| e.stop_propagation(),
            onkeydown: move |e: KeyboardEvent| {
                if e.key() == Key::Escape { state.renaming.set(None); }
            },
            onchange: move |e| {
                if *state.renaming.peek() == Some(id) {
                    state.renaming.set(None);
                    on_commit.call(e.value());
                }
            },
            onblur: move |_| if *state.renaming.peek() == Some(id) { state.renaming.set(None) },
        }
    }
}

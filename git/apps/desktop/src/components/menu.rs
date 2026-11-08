//! Context menu (fixed at the pointer) with a click-away backdrop.

use dioxus::prelude::*;

#[derive(Debug, Clone, PartialEq)]
pub struct MenuItem {
    pub label: String,
    pub danger: bool,
    /// Renders a separator above this item.
    pub separator: bool,
}

impl MenuItem {
    pub fn new(label: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            danger: false,
            separator: false,
        }
    }

    #[must_use]
    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    #[must_use]
    pub fn sep(mut self) -> Self {
        self.separator = true;
        self
    }
}

/// `on_pick` receives the index of the chosen item.
#[component]
pub fn ContextMenu(
    pos: (f64, f64),
    items: Vec<MenuItem>,
    on_pick: EventHandler<usize>,
    on_close: EventHandler<()>,
) -> Element {
    let (x, y) = pos;
    // Keep the menu on screen near the bottom/right edges.
    let style = format!(
        "left: min({x}px, calc(100vw - 15rem)); top: min({y}px, calc(100vh - {}rem));",
        items.len() as f64 * 1.9 + 1.0
    );
    rsx! {
        div { class: "fixed inset-0 z-30", onclick: move |_| on_close.call(()),
            oncontextmenu: move |e| { e.prevent_default(); on_close.call(()); } }
        div {
            class: "fixed z-40 min-w-56 rounded-md border bg-popover py-1 text-xs text-popover-foreground shadow-lg",
            style,
            role: "menu",
            for (i, item) in items.into_iter().enumerate() {
                div { key: "{i}",
                if item.separator { div { class: "my-1 h-px bg-border" } }
                button {
                    class: if item.danger { "block w-full px-3 py-1.5 text-left hover:bg-accent text-destructive" } else { "block w-full px-3 py-1.5 text-left hover:bg-accent" },
                    role: "menuitem",
                    onclick: move |e| { e.stop_propagation(); on_close.call(()); on_pick.call(i); },
                    "{item.label}"
                }
                }
            }
        }
    }
}

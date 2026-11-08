use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragAxis {
    /// Left sidebar (grows to the right).
    Sidebar,
    /// Right detail panel (grows to the left).
    Detail,
}

/// Active drag: pointer origin and the size at drag start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragState {
    pub axis: DragAxis,
    pub origin: f64,
    pub start_size: f64,
}

impl DragState {
    #[must_use]
    pub fn size_at(&self, x: f64) -> f64 {
        match self.axis {
            DragAxis::Sidebar => self.start_size + (x - self.origin),
            DragAxis::Detail => self.start_size + (x - self.origin),
        }
    }
}

#[component]
pub fn Splitter(axis: DragAxis, size: f64, on_start: EventHandler<DragState>) -> Element {
    let label = match axis {
        DragAxis::Sidebar => "Resize sidebar",
        DragAxis::Detail => "Resize list",
    };
    rsx! {
        div {
            class: "w-1 shrink-0 cursor-col-resize bg-border hover:bg-ring transition-colors",
            role: "separator",
            "aria-label": label,
            onmousedown: move |e: MouseEvent| {
                e.prevent_default();
                on_start.call(DragState { axis, origin: e.client_coordinates().x, start_size: size });
            },
        }
    }
}

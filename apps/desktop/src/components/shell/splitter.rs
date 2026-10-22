use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DragAxis {
    /// Resizes the sidebar (drag left/right).
    Sidebar,
    /// Resizes the response panel (drag up/down).
    Results,
}

/// Active drag: pointer origin and the size at drag start.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DragState {
    pub axis: DragAxis,
    pub origin: f64,
    pub start_size: f64,
}

impl DragState {
    /// New size for pointer position `pos` (client x for sidebar, y for results).
    #[must_use]
    pub fn size_at(&self, pos: f64) -> f64 {
        match self.axis {
            DragAxis::Sidebar => self.start_size + (pos - self.origin),
            DragAxis::Results => self.start_size - (pos - self.origin),
        }
    }
}

#[component]
pub fn Splitter(axis: DragAxis, size: f64, on_start: EventHandler<DragState>) -> Element {
    let class = match axis {
        DragAxis::Sidebar => {
            "w-1 shrink-0 cursor-col-resize bg-border hover:bg-ring transition-colors"
        }
        DragAxis::Results => {
            "h-1 shrink-0 cursor-row-resize bg-border hover:bg-ring transition-colors"
        }
    };
    let label = match axis {
        DragAxis::Sidebar => "Resize sidebar",
        DragAxis::Results => "Resize response panel",
    };
    rsx! {
        div {
            class,
            role: "separator",
            "aria-label": label,
            onmousedown: move |e: MouseEvent| {
                e.prevent_default();
                let p = e.client_coordinates();
                let origin = if axis == DragAxis::Sidebar { p.x } else { p.y };
                on_start.call(DragState { axis, origin, start_size: size });
            },
        }
    }
}

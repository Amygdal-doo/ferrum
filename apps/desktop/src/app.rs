//! Root component and window layout.

use crate::commands::{collections, send, workspace};
use crate::components::dialogs::DialogHost;
use crate::components::request::RequestPane;
use crate::components::response::ResponsePanel;
use crate::components::shell::{DragAxis, DragState, Header, NoticeHost, Splitter, StatusBar};
use crate::components::sidebar::Sidebar;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumapi_application::AppServices;

#[component]
pub fn App() -> Element {
    let services = use_context::<AppServices>();
    let mut state = use_context_provider(AppState::new);
    let mut drag = use_signal(|| None::<DragState>);

    let svc = services.clone();
    use_future(move || workspace::bootstrap(state, svc.clone()));
    let svc = services.clone();
    use_future(move || workspace::autosave_loop(state, svc.clone()));

    let sidebar_width = state.sidebar_width();
    let results_height = *state.results_height.read();
    let root_class = if state.is_dark() {
        "dark h-full"
    } else {
        "h-full"
    };
    let cursor = match *drag.read() {
        Some(DragState {
            axis: DragAxis::Sidebar,
            ..
        }) => "cursor-col-resize",
        Some(DragState {
            axis: DragAxis::Results,
            ..
        }) => "cursor-row-resize",
        None => "",
    };
    let svc_drag = services.clone();
    let svc_keys = services.clone();

    rsx! {
        div { class: "{root_class}",
            div {
                class: "flex h-full flex-col bg-background text-foreground select-none {cursor}",
                onkeydown: move |e: KeyboardEvent| {
                    let m = e.modifiers();
                    if !(m.meta() || m.ctrl()) || state.dialog.peek().is_some() {
                        return;
                    }
                    match e.key() {
                        Key::Enter => {
                            e.prevent_default();
                            send::send_or_cancel_active(state, svc_keys.clone());
                        }
                        Key::Character(c) if c.eq_ignore_ascii_case("s") => {
                            e.prevent_default();
                            if let Some(id) = *state.active_tab.peek() {
                                collections::save_tab(state, svc_keys.clone(), id);
                            }
                        }
                        _ => {}
                    }
                },
                onmousemove: move |e: MouseEvent| {
                    let Some(d) = *drag.read() else { return };
                    let p = e.client_coordinates();
                    match d.axis {
                        DragAxis::Sidebar => state.settings.write().sidebar_width = d.size_at(p.x).clamp(200.0, 560.0),
                        DragAxis::Results => state.results_height.set(d.size_at(p.y).clamp(120.0, 900.0)),
                    }
                },
                onmouseup: move |_| {
                    let current = *drag.peek();
                    if let Some(d) = current {
                        drag.set(None);
                        if d.axis == DragAxis::Sidebar {
                            workspace::save_settings(state, svc_drag.clone());
                        }
                    }
                },
                Header {}
                if *state.loaded.read() {
                    div { class: "flex min-h-0 flex-1",
                        div { class: "shrink-0 overflow-hidden", style: "width: {sidebar_width}px", Sidebar {} }
                        Splitter { axis: DragAxis::Sidebar, size: sidebar_width, on_start: move |d| drag.set(Some(d)) }
                        main { class: "flex min-w-0 flex-1 flex-col",
                            div { class: "min-h-0 flex-1", RequestPane {} }
                            Splitter { axis: DragAxis::Results, size: results_height, on_start: move |d| drag.set(Some(d)) }
                            div { class: "shrink-0", style: "height: {results_height}px", ResponsePanel {} }
                        }
                    }
                } else {
                    div { class: "flex flex-1 items-center justify-center text-sm text-muted-foreground", "Loading workspace…" }
                }
                StatusBar {}
            }
            DialogHost {}
            NoticeHost {}
        }
    }
}

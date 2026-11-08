//! Root component and window layout.

use crate::commands::workspace;
use crate::components::connection::Sidebar;
use crate::components::dialogs::DialogHost;
use crate::components::result_grid::ResultsPanel;
use crate::components::shell::{DragAxis, DragState, Header, NoticeHost, Splitter, StatusBar};
use crate::components::sql_editor::EditorPane;
use crate::components::table_view::TableView;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrumdata_application::AppServices;

#[component]
pub fn App() -> Element {
    let services = use_context::<AppServices>();
    let mut state = use_context_provider(AppState::new);
    let mut drag = use_signal(|| None::<DragState>);

    let svc = services.clone();
    use_future(move || workspace::bootstrap(state, svc.clone()));
    let svc = services.clone();
    use_future(move || workspace::autosave_loop(state, svc.clone()));

    let layout = state.settings.read().layout.clamped();
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
    let table_tab = state.active_tab().is_some_and(|t| t.table.is_some());

    rsx! {
        div { class: "{root_class}",
            div {
                class: "flex h-full flex-col bg-background text-foreground select-none {cursor}",
                onmousemove: move |e: MouseEvent| {
                    let Some(d) = *drag.read() else { return };
                    let p = e.client_coordinates();
                    let mut settings = state.settings.write();
                    match d.axis {
                        DragAxis::Sidebar => settings.layout.sidebar_width = d.size_at(p.x),
                        DragAxis::Results => settings.layout.results_height = d.size_at(p.y),
                    }
                    settings.layout = settings.layout.clamped();
                },
                onmouseup: move |_| {
                    if drag.peek().is_some() {
                        drag.set(None);
                        workspace::save_settings(state, svc_drag.clone());
                    }
                },
                Header {}
                if *state.loaded.read() {
                    div { class: "flex min-h-0 flex-1",
                        div { class: "shrink-0 overflow-hidden", style: "width: {layout.sidebar_width}px", Sidebar {} }
                        Splitter { axis: DragAxis::Sidebar, size: layout.sidebar_width, on_start: move |d| drag.set(Some(d)) }
                        main { class: "flex min-w-0 flex-1 flex-col",
                            if table_tab {
                                // Tab bar stays visible; the table view replaces editor + results.
                                div { class: "flex min-h-0 flex-1 flex-col",
                                    EditorPane { tabs_only: true }
                                    div { class: "min-h-0 flex-1", TableView {} }
                                }
                            } else {
                                div { class: "min-h-0 flex-1", EditorPane { tabs_only: false } }
                                Splitter { axis: DragAxis::Results, size: layout.results_height, on_start: move |d| drag.set(Some(d)) }
                                div { class: "shrink-0", style: "height: {layout.results_height}px", ResultsPanel {} }
                            }
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

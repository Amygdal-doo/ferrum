//! Root component and window layout (Tower style):
//! titlebar toolbar / sidebar | list | details.

use crate::commands::{repo, workspace};
use crate::components::detail::{MiddleColumn, RightColumn};
use crate::components::dialogs::DialogHost;
use crate::components::home::Home;
use crate::components::shell::{DragAxis, DragState, NoticeHost, Splitter, Toolbar};
use crate::components::sidebar::Sidebar;
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumgit_application::AppServices;

#[component]
pub fn App() -> Element {
    let services = use_context::<AppServices>();
    let mut state = use_context_provider(AppState::new);
    let mut drag = use_signal(|| None::<DragState>);

    let svc = services.clone();
    use_future(move || workspace::bootstrap(state, svc.clone()));
    let svc = services.clone();
    use_future(move || repo::poll_loop(state, svc.clone()));

    let sidebar_width = state.sidebar_width();
    let list_width = state.detail_width();
    let root_class = if state.is_dark() {
        "dark h-full"
    } else {
        "h-full"
    };
    let cursor = if drag.read().is_some() {
        "cursor-col-resize"
    } else {
        ""
    };
    let active = *state.active.read();
    let (svc_drag, svc_keys) = (services.clone(), services.clone());

    rsx! {
        div { class: "{root_class}",
            div {
                class: "flex h-full flex-col bg-background text-foreground select-none {cursor}",
                tabindex: "-1",
                onkeydown: move |e: KeyboardEvent| {
                    if state.dialog.peek().is_some() { return; }
                    let m = e.modifiers();
                    let cmd = m.meta() || m.ctrl();
                    match e.key() {
                        Key::Character(c) if cmd && (c.eq_ignore_ascii_case("o") || c.eq_ignore_ascii_case("p")) => {
                            e.prevent_default();
                            state.dialog.set(Some(Dialog::QuickOpen));
                        }
                        _ => {}
                    }
                    let Some(id) = *state.active.peek() else { return };
                    match e.key() {
                        Key::Escape => state.update_view(id, |v| v.diff = None),
                        Key::Character(c) if cmd && c.eq_ignore_ascii_case("r") => {
                            e.prevent_default();
                            repo::load(state, svc_keys.clone(), id);
                        }
                        Key::Character(c) if cmd && c == "[" => repo::go(state, svc_keys.clone(), id, true),
                        Key::Character(c) if cmd && c == "]" => repo::go(state, svc_keys.clone(), id, false),
                        _ => {}
                    }
                },
                onmousemove: move |e: MouseEvent| {
                    let Some(d) = *drag.read() else { return };
                    let x = e.client_coordinates().x;
                    match d.axis {
                        DragAxis::Sidebar => state.settings.write().sidebar_width = d.size_at(x).clamp(180.0, 480.0),
                        DragAxis::Detail => state.settings.write().detail_width = d.size_at(x).clamp(300.0, 900.0),
                    }
                },
                onmouseup: move |_| {
                    if drag.peek().is_some() {
                        drag.set(None);
                        workspace::save_settings(state, svc_drag.clone());
                    }
                },
                Toolbar {}
                if !*state.loaded.read() {
                    div { class: "flex flex-1 items-center justify-center text-sm text-muted-foreground", "Loading…" }
                } else if active.is_none() {
                    Home {}
                } else {
                    div { class: "flex min-h-0 flex-1",
                        div { class: "shrink-0 overflow-hidden", style: "width: {sidebar_width}px", Sidebar {} }
                        Splitter { axis: DragAxis::Sidebar, size: sidebar_width, on_start: move |d| drag.set(Some(d)) }
                        div { class: "shrink-0 overflow-hidden", style: "width: {list_width}px", MiddleColumn {} }
                        Splitter { axis: DragAxis::Detail, size: list_width, on_start: move |d| drag.set(Some(d)) }
                        RightColumn {}
                    }
                }
            }
            DialogHost {}
            NoticeHost {}
        }
    }
}

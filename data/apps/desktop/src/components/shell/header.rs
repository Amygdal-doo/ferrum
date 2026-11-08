use crate::commands::{query, workspace};
use crate::components::env_dot_class;
use crate::components::rust_ui::badge::{Badge, BadgeVariant};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::components::rust_ui::kbd::Kbd;
use crate::state::AppState;
use dioxus::prelude::*;
use ferrum_application::AppServices;
use ferrum_domain::{ProfileId, Theme};

#[component]
pub fn Header() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let tab = state.active_tab();
    let profile = state.active_profile();
    let running = tab
        .as_ref()
        .and_then(|t| state.runs.read().get(&t.id).map(|r| !r.state.is_terminal()))
        .unwrap_or(false);
    let theme = state.settings.read().theme;
    let theme_label = match theme {
        Theme::System => "Theme: System",
        Theme::Light => "Theme: Light",
        Theme::Dark => "Theme: Dark",
    };
    let selected = profile
        .as_ref()
        .map(|p| p.id.to_string())
        .unwrap_or_default();
    let svc_run = services.clone();
    let svc_cancel = services.clone();
    let svc_theme = services.clone();

    rsx! {
        header { class: "flex h-12 shrink-0 items-center gap-3 border-b bg-sidenav px-3",
            span { class: "text-sm font-semibold tracking-tight", "Ferrum" }
            div { class: "mx-2 h-5 w-px bg-border" }
            if let Some(p) = &profile {
                span { class: "size-2.5 rounded-full {env_dot_class(p.env)}", title: "{p.env.label()}" }
            }
            select {
                class: "h-8 min-w-56 rounded-md border bg-background px-2 text-sm",
                "aria-label": "Connection for this tab",
                value: "{selected}",
                disabled: tab.is_none(),
                onchange: move |e| {
                    let Some(tab_id) = *state.active_tab.peek() else { return };
                    let id = e.value().parse::<ProfileId>().ok();
                    state.update_tab(tab_id, |t| { t.profile_id = id; t.unsaved = true; });
                },
                option { value: "", "No connection" }
                for p in state.profiles.read().iter() {
                    option { key: "{p.id}", value: "{p.id}", selected: p.id.to_string() == selected, "{p.display_name}" }
                }
            }
            if let Some(p) = &profile {
                if p.env.is_production() {
                    Badge { variant: BadgeVariant::Destructive, "PRODUCTION" }
                }
                if p.read_only {
                    Badge { variant: BadgeVariant::Info, "Read only" }
                }
            }
            div { class: "flex-1" }
            Button {
                size: ButtonSize::Sm,
                disabled: running || profile.is_none(),
                title: "Run selection or current statement (⌘/Ctrl+Enter)",
                onclick: move |_| { spawn(query::run_from_editor(state, svc_run.clone())); },
                "Run ▷"
                Kbd { class: "bg-primary-foreground/20 text-primary-foreground", "⌘↵" }
            }
            Button {
                size: ButtonSize::Sm,
                variant: ButtonVariant::Outline,
                disabled: !running,
                title: "Cancel the running query",
                onclick: move |_| query::cancel_active(state, &svc_cancel),
                "Cancel ■"
            }
            Button {
                size: ButtonSize::Sm,
                variant: ButtonVariant::Ghost,
                title: "Switch theme",
                onclick: move |_| {
                    let next = match theme { Theme::System => Theme::Light, Theme::Light => Theme::Dark, Theme::Dark => Theme::System };
                    state.settings.write().theme = next;
                    workspace::save_settings(state, svc_theme.clone());
                },
                "{theme_label}"
            }
        }
    }
}

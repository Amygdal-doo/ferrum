use crate::commands::{environments, workspace};
use crate::components::rust_ui::button::{Button, ButtonSize, ButtonVariant};
use crate::state::{AppState, Dialog};
use dioxus::prelude::*;
use ferrumapi_application::AppServices;
use ferrumapi_domain::{EnvironmentId, Theme};
use uuid::Uuid;

#[component]
pub fn Header() -> Element {
    let mut state = use_context::<AppState>();
    let services = use_context::<AppServices>();
    let theme = state.settings.read().theme;
    let theme_label = match theme {
        Theme::System => "Theme: System",
        Theme::Light => "Theme: Light",
        Theme::Dark => "Theme: Dark",
    };
    let selected = (*state.active_env.read())
        .map(|id| id.to_string())
        .unwrap_or_default();
    let envs = state.environments.read().clone();
    let has_env = !selected.is_empty();

    rsx! {
        header { class: "flex h-12 shrink-0 items-center gap-3 border-b bg-sidenav px-3",
            span { class: "text-sm font-semibold tracking-tight", "FerrumAPI" }
            div { class: "mx-2 h-5 w-px bg-border" }
            span { class: "text-xs text-muted-foreground", "Environment" }
            select {
                class: "h-8 min-w-48 rounded-md border bg-background px-2 text-sm",
                "aria-label": "Active environment",
                onchange: move |e| {
                    let id = Uuid::parse_str(&e.value()).ok().map(EnvironmentId);
                    environments::set_active(state, id);
                },
                option { value: "", selected: !has_env, "No environment" }
                for env in envs {
                    option { key: "{env.id}", value: "{env.id}", selected: env.id.to_string() == selected, "{env.name}" }
                }
            }
            if let Some(id) = *state.active_env.read() {
                Button {
                    size: ButtonSize::Sm,
                    variant: ButtonVariant::Ghost,
                    class: "h-7 px-2 text-xs",
                    title: "Edit the active environment",
                    onclick: move |_| state.dialog.set(Some(Dialog::EditEnvironment(id))),
                    "Edit"
                }
            }
            div { class: "flex-1" }
            Button {
                size: ButtonSize::Sm,
                variant: ButtonVariant::Outline,
                title: "Import a Postman collection or a cURL command",
                onclick: move |_| state.dialog.set(Some(Dialog::Import)),
                "Import"
            }
            Button {
                size: ButtonSize::Sm,
                variant: ButtonVariant::Ghost,
                title: "Switch theme",
                onclick: move |_| {
                    let next = match theme { Theme::System => Theme::Light, Theme::Light => Theme::Dark, Theme::Dark => Theme::System };
                    state.settings.write().theme = next;
                    workspace::save_settings(state, services.clone());
                },
                "{theme_label}"
            }
        }
    }
}

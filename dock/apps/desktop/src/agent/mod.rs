//! The Agent pane: FerrumChat beside the cards, without a project. Before
//! each message it learns what the window shows (the engine, the section,
//! the open container, the containers) and gets the connected engine for
//! its tools (see [`tools`]).

mod tools;

use std::rc::Rc;

use ferrumagent_ui::core::host::AppHost;
use ferrumagent_ui::{AgentPane, ChatView};
use gpui::{App, Context, Window};

use crate::app::FerrumContainers;
use crate::state::{AppState, EngineStatus};
pub use tools::EngineSlot;

/// Containers listed in the context at most (the tool lists them all).
const CONTEXT_CONTAINERS: usize = 40;

const ABOUT: &str = "FerrumContainers manages Docker containers, images, volumes, networks and \
                     compose projects on the user's machine.";

impl FerrumContainers {
    /// The top-right Agent toggle: shows the chat (made the first time) or
    /// hides it.
    pub fn toggle_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let slot = self.agent_engine.clone();
        let services = self.services.clone();
        let weak = cx.weak_entity();
        self.agent.toggle(window, cx, move |window, cx| {
            let host = AppHost {
                tools: tools::tools(&slot, &services),
                ..AppHost::new("FerrumContainers", ABOUT)
            };
            let context = Rc::new(move |cx: &mut App| {
                weak.update(cx, |app, _| {
                    app.agent_engine.set(app.engine());
                    context(&app.status, &app.state)
                })
                .unwrap_or_default()
            });
            ChatView::without_project(host, AgentPane::options(Some(context)), window, cx)
        });
        cx.notify();
    }

    /// The Agent toggle, for the header that ends with the account.
    pub fn agent_toggle(&self, cx: &mut Context<Self>) -> gpui::AnyElement {
        use ferrum_ui::components::Clickable as _;
        use gpui::IntoElement as _;
        ferrumagent_ui::agent_toggle("toggle-agent", self.agent.is_shown())
            .on_click(cx.listener(|app, _, window, cx| app.toggle_agent(window, cx)))
            .into_any_element()
    }
}

/// What the window shows, for the chat.
pub fn context(status: &EngineStatus, state: &AppState) -> String {
    let mut out = String::new();
    match status {
        EngineStatus::Connecting => out.push_str("Engine: connecting…\n"),
        EngineStatus::Offline(error) => out.push_str(&format!("Engine: offline ({error})\n")),
        EngineStatus::Connected(c) => {
            let name = c
                .info
                .platform
                .clone()
                .unwrap_or_else(|| format!("Docker {}", c.info.version));
            out.push_str(&format!(
                "Engine: {name} via {} ({}/{})\n",
                c.candidate.source, c.info.os, c.info.arch
            ));
        }
    }
    if let Some(project) = &state.project {
        let files: Vec<String> = project
            .files
            .iter()
            .map(|f| f.path.display().to_string())
            .collect();
        out.push_str(&format!("Project folder: {}", project.root.display()));
        if !files.is_empty() {
            out.push_str(&format!(" (compose files: {})", files.join(", ")));
        }
        out.push('\n');
    }
    out.push_str(&format!("Section shown: {}", state.section.title()));
    if !state.filter.trim().is_empty() {
        out.push_str(&format!(" (filtered by \"{}\")", state.filter.trim()));
    }
    out.push('\n');
    if let Some(detail) = &state.detail {
        match (&detail.project, state.container(&detail.id)) {
            (Some(project), _) => out.push_str(&format!(
                "Open: compose project {} ({:?} tab)\n",
                project.name, detail.tab
            )),
            (None, Some(c)) => out.push_str(&format!(
                "Open: container {} — {}, {} ({:?} tab)\n",
                c.name,
                c.state.label(),
                c.status,
                detail.tab
            )),
            (None, None) => {}
        }
        if let Some(error) = &detail.log_error {
            out.push_str(&format!("Log error shown: {error}\n"));
        }
    }
    if !state.containers.is_empty() {
        out.push_str(&format!(
            "Containers: {} ({} running)\n",
            state.containers.len(),
            state.running_count()
        ));
        for c in state.containers.iter().take(CONTEXT_CONTAINERS) {
            out.push_str(&format!(
                "- {}: {} — {}\n",
                c.name,
                c.state.label(),
                c.status
            ));
        }
        if state.containers.len() > CONTEXT_CONTAINERS {
            out.push_str("- … (containers_list has them all)\n");
        }
    }
    if let Some(notice) = &state.notice {
        out.push_str(&format!("Last message: {}\n", notice.message));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{DetailState, Notice};
    use ferrumdock_engine::fake::container;

    #[test]
    fn the_context_says_what_the_window_shows() {
        let state = AppState {
            containers: vec![
                container("aaa111", "web-app-1", Some(("web", "app")), "running"),
                container("bbb222", "web-db-1", Some(("web", "db")), "exited"),
            ],
            filter: "web".into(),
            detail: Some(DetailState::new("bbb222".into())),
            notice: Some(Notice {
                message: "Stopped web-db-1".into(),
                error: false,
            }),
            ..AppState::default()
        };
        let text = context(&EngineStatus::Offline("socket refused".into()), &state);
        assert!(text.contains("Engine: offline (socket refused)"), "{text}");
        assert!(text.contains("Section shown: Containers (filtered by \"web\")"));
        assert!(text.contains("Open: container web-db-1 — exited"), "{text}");
        assert!(text.contains("Containers: 2 (1 running)"));
        assert!(text.contains("- web-app-1: running"));
        assert!(text.contains("Last message: Stopped web-db-1"));

        let empty = context(&EngineStatus::Connecting, &AppState::default());
        assert_eq!(empty, "Engine: connecting…\nSection shown: Containers\n");
    }
}

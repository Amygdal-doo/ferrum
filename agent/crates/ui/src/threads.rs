//! The thread list (replaces the chat column while open), in sections by
//! status: Needs you, Done, Working, Idle. Each row: the agent's logo,
//! title, project and agent, time since its last change and a status mark.

use std::time::Duration;

use chrono::Utc;
use ferrum_ui::prelude::*;
use ferrumagent_core::session::AgentChoice;
use ferrumagent_core::status::{StatusEntry, ThreadStatus, short_ago};
use ferrumagent_core::store::ThreadSummary;
use gpui::{Animation, AnimationExt, AnyElement, Context, Transformation, Window, percentage};

use crate::live::Target;
use crate::view::ChatView;

/// The logo of a thread's agent (`provider/model` or `acp:<id>`).
pub(crate) fn agent_logo(agent: &str) -> Option<ferrum_ui::AgentLogo> {
    match AgentChoice::parse(agent)? {
        AgentChoice::Native { model, .. } => ferrum_ui::AgentLogo::for_model(&model),
        AgentChoice::Acp { agent } => {
            ferrum_ui::AgentLogo::for_program(agent.split(['-', ' ']).next().unwrap_or(&agent))
        }
    }
}

/// A thread's agent logo, or the plain `>_` tile.
pub fn thread_logo(agent: &str, size: Pixels, cx: &App) -> AnyElement {
    match agent_logo(agent) {
        Some(logo) => ferrum_ui::agent_logo(logo, size, cx).into_any_element(),
        None => ferrum_ui::terminal_tile(size, cx).into_any_element(),
    }
}

/// The right-hand status mark: ⚠ Needs you, a blue dot for Done (red when
/// it failed), a spinner while Working, a hollow circle when Idle.
pub fn status_indicator(entry: Option<&StatusEntry>, cx: &App) -> AnyElement {
    let status = entry.map_or(ThreadStatus::Idle, |e| e.status);
    let dot = |color: gpui::Hsla| div().flex_none().size(px(7.)).rounded_full().bg(color);
    let mark = match status {
        ThreadStatus::NeedsYou => Icon::new(IconName::Warning)
            .size(IconSize::XSmall)
            .color(Color::Warning)
            .into_any_element(),
        ThreadStatus::Done if entry.is_some_and(|e| e.error) => {
            dot(Color::Error.color(cx)).into_any_element()
        }
        ThreadStatus::Done => dot(Color::Accent.color(cx)).into_any_element(),
        ThreadStatus::Working => Icon::new(IconName::LoadCircle)
            .size(IconSize::XSmall)
            .color(Color::Accent)
            .with_animation(
                "thread-working",
                Animation::new(Duration::from_secs(1)).repeat(),
                |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
            )
            .into_any_element(),
        ThreadStatus::Idle => div()
            .flex_none()
            .size(px(7.))
            .rounded_full()
            .border_1()
            .border_color(Color::Muted.color(cx))
            .into_any_element(),
    };
    div()
        .flex_none()
        .w(px(14.))
        .flex()
        .justify_center()
        .items_center()
        .child(mark)
        .into_any_element()
}

impl ChatView {
    /// "claude-sonnet-5-5", "Claude Code".
    fn agent_name(&self, agent: &str) -> String {
        match AgentChoice::parse(agent) {
            Some(AgentChoice::Native { model, .. }) => model,
            Some(AgentChoice::Acp { agent }) => self
                .session
                .as_ref()
                .and_then(|s| s.acp_agents().iter().find(|a| a.id == agent))
                .map_or(agent, |a| a.name.clone()),
            None => agent.to_string(),
        }
    }

    fn thread_row(
        &self,
        thread: &ThreadSummary,
        current: bool,
        project: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let entry = self.statuses.get(&thread.id);
        let ago = short_ago(
            self.statuses.changed(&thread.id, thread.updated),
            Utc::now(),
        );
        let subtitle = match entry.filter(|e| !e.summary.is_empty()) {
            Some(e) if e.status != ThreadStatus::Idle => e.summary.clone(),
            _ => format!("{project} · {}", self.agent_name(&thread.agent)),
        };
        let title = if thread.title.trim().is_empty() {
            "New thread".to_string()
        } else {
            thread.title.clone()
        };
        let (id, delete_id) = (thread.id.clone(), thread.id.clone());
        let key = gpui::ElementId::Name(format!("chat-thread-{}", thread.id).into());
        let delete_key = gpui::ElementId::Name(format!("chat-thread-delete-{}", thread.id).into());
        ListItem::new(key)
            .rounded()
            .toggle_state(current)
            .start_slot(thread_logo(&thread.agent, px(16.), cx))
            .child(
                v_flex()
                    .min_w_0()
                    .child(
                        Label::new(title)
                            .size(LabelSize::Small)
                            .single_line()
                            .truncate(),
                    )
                    .child(
                        Label::new(subtitle)
                            .size(LabelSize::XSmall)
                            .color(Color::Muted)
                            .single_line()
                            .truncate(),
                    ),
            )
            .end_slot(
                h_flex()
                    .flex_none()
                    .gap_1()
                    .child(Label::new(ago).size(LabelSize::XSmall).color(Color::Muted))
                    .child(status_indicator(entry, cx))
                    .child(
                        IconButton::new(delete_key, IconName::Trash)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Delete thread"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.delete_thread(&delete_id, cx);
                            })),
                    ),
            )
            .on_click(cx.listener(move |this, _, window, cx| {
                this.switch_to(Target::Open(id.clone()), cx);
                this.show_threads = false;
                this.refocus(window, cx);
                cx.notify();
            }))
    }

    pub(crate) fn render_threads(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let current = self.thread.id.clone();
        let project = self.project.file_name().map_or_else(
            || self.project.display().to_string(),
            |n| n.to_string_lossy().to_string(),
        );
        let threads = self.threads.clone();
        let groups = self
            .statuses
            .group(&threads, |t| t.id.as_str(), |t| t.updated);
        let mut list = v_flex()
            .id("chat-thread-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_1()
            .gap_0p5();
        if threads.is_empty() {
            list = list.child(
                div().p_4().child(
                    Label::new("No saved threads for this project yet.")
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                ),
            );
        }
        for (status, rows) in groups {
            list = list.child(
                h_flex()
                    .px_2()
                    .pt_1p5()
                    .pb_0p5()
                    .justify_between()
                    .child(
                        Label::new(status.label())
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(rows.len().to_string())
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            );
            for thread in rows {
                list = list.child(self.thread_row(thread, thread.id == current, &project, cx));
            }
        }
        list
    }
}

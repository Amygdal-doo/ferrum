//! The thread list (replaces the chat column while open): New thread on
//! top, then the threads in sections by status: Needs you, Done, Working,
//! Idle. Each row: the agent's logo, title (renamed in place), project and
//! agent, time since its last change, a status mark, Rename and Delete.

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
        // The model's maker (a Qwen model on Ollama shows Qwen), else the
        // provider's own logo.
        AgentChoice::Native { provider, model } => {
            ferrum_ui::AgentLogo::for_provider_model(&provider, &model)
        }
        AgentChoice::Acp { agent } => ferrum_ui::AgentLogo::for_program(&agent).or_else(|| {
            ferrum_ui::AgentLogo::for_program(agent.split(['-', ' ']).next().unwrap_or(&agent))
        }),
    }
}

/// A thread's agent logo, or the plain `>_` tile.
pub fn thread_logo(agent: &str, size: Pixels, cx: &App) -> AnyElement {
    ferrum_ui::agent_logo_or_tile(agent_logo(agent), size, cx)
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
        let (id, delete_id, rename_id) = (thread.id.clone(), thread.id.clone(), thread.id.clone());
        let key = gpui::ElementId::Name(format!("chat-thread-{}", thread.id).into());
        let delete_key = gpui::ElementId::Name(format!("chat-thread-delete-{}", thread.id).into());
        let rename_key = gpui::ElementId::Name(format!("chat-thread-rename-{}", thread.id).into());
        let title_field = self
            .renaming_row
            .as_ref()
            .filter(|(renamed, _, _)| *renamed == thread.id)
            .map(|(_, input, _)| input.clone());
        let title = match title_field {
            Some(input) => div()
                .px_1()
                .rounded(px(4.))
                .border_1()
                .border_color(cx.theme().colors().border_focused)
                .child(input)
                .into_any_element(),
            None => Label::new(title)
                .size(LabelSize::Small)
                .single_line()
                .truncate()
                .into_any_element(),
        };
        ListItem::new(key)
            .rounded()
            .toggle_state(current)
            .start_slot(thread_logo(&thread.agent, px(16.), cx))
            .child(
                v_flex().min_w_0().child(title).child(
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
                        IconButton::new(rename_key, IconName::Pencil)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Rename thread"))
                            .on_click(cx.listener(move |this, _, window, cx| {
                                this.start_row_rename(&rename_id, window, cx);
                            })),
                    )
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
                // A click into the row's title field is not a switch.
                if this.renaming_row.as_ref().is_some_and(|(r, _, _)| *r == id) {
                    return;
                }
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
            .gap_0p5()
            .child(
                h_flex().px_1().pt_1().pb_0p5().child(
                    Button::new("chat-thread-list-new", "New thread")
                        .style(ButtonStyle::Subtle)
                        .size(ButtonSize::Compact)
                        .start_icon(Icon::new(IconName::Plus).size(IconSize::Small))
                        .tooltip(Tooltip::text("New thread (⌘⌥N)"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.new_thread(&crate::actions::NewThread, window, cx)
                        })),
                ),
            );
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

#[cfg(test)]
mod logo_tests {
    use super::agent_logo;
    use ferrum_ui::AgentLogo::*;

    #[test]
    fn threads_show_the_model_maker_or_the_agent() {
        assert_eq!(agent_logo("anthropic/claude-sonnet-5-5"), Some(Claude));
        assert_eq!(agent_logo("openai/gpt-5"), Some(OpenAi));
        assert_eq!(agent_logo("openai/gpt-5-codex"), Some(Codex));
        assert_eq!(agent_logo("ollama/qwen3-coder:30b"), Some(Qwen));
        assert_eq!(agent_logo("ollama/deepseek-r1:14b"), Some(DeepSeek));
        assert_eq!(agent_logo("ollama/my-finetune"), Some(Ollama));
        assert_eq!(agent_logo("openrouter/moonshotai/kimi-k2"), Some(Kimi));
        assert_eq!(agent_logo("openrouter/z-ai/glm-4.6"), Some(Glm));
        assert_eq!(agent_logo("acp:claude-code"), Some(Claude));
        assert_eq!(agent_logo("acp:codex"), Some(Codex));
        assert_eq!(agent_logo("acp:gemini"), Some(Gemini));
        assert_eq!(agent_logo("acp:qwen-code"), Some(Qwen));
        assert_eq!(agent_logo("acp:my-agent"), None);
        assert_eq!(agent_logo("garbage"), None);
    }
}

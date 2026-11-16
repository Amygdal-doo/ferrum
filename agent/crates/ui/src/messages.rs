//! The message list: user messages (with their checkpoint), assistant
//! text, thinking, tool cards, plans and errors without bubbles; after a
//! turn, copy / retry and the summary of changed files; the plan-ready
//! card.

use ferrum_ui::prelude::*;
use ferrumagent_core::modes::Mode;
use ferrumagent_core::provider::Role;
use ferrumagent_core::thread::{Block, PlanEntry, ThreadMessage};
use gpui::{Context, Window};

use crate::actions::*;
use crate::markdown;
use crate::view::ChatView;

impl ChatView {
    pub(crate) fn render_messages(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let messages = self.thread.messages.clone();
        let last_is_user = messages.last().is_none_or(|m| m.role == Role::User);
        let turn_done = !self.busy && !last_is_user;
        let show_plan = self.thread.mode == Mode::Plan && !self.busy && self.thread.plan.is_some();
        div()
            .id("chat-messages")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.scroll)
            .on_scroll(cx.listener(|this, event: &gpui::ScrollEvent, _, _| {
                let up = event.delta.pixel_delta(px(16.)).y > px(0.);
                if up {
                    this.follow = false;
                } else {
                    let offset = -this.scroll.offset().y;
                    this.follow = offset + px(48.) >= this.scroll.max_offset().y;
                }
            }))
            .child(
                v_flex()
                    .p_2()
                    .gap_3()
                    .when(messages.is_empty() && self.session.is_some(), |this| {
                        this.child(self.render_empty(cx))
                    })
                    .when(
                        self.session.is_none() && self.load_error.is_none(),
                        |this| {
                            let label = if self.advice {
                                "Starting"
                            } else {
                                "Opening project"
                            };
                            this.child(div().p_4().child(LoadingLabel::new(label)))
                        },
                    )
                    .children(messages.iter().map(|m| match m.role {
                        Role::User => self.render_user(m, cx).into_any_element(),
                        Role::Assistant => self.render_assistant(m, window, cx).into_any_element(),
                    }))
                    .when(self.busy && last_is_user, |this| {
                        this.child(
                            div()
                                .px_1()
                                .child(LoadingLabel::new("Thinking").color(Color::Muted)),
                        )
                    })
                    .when(turn_done, |this| this.child(self.render_turn_actions(cx)))
                    .when(turn_done && !self.changes.is_empty(), |this| {
                        this.child(self.render_change_summary(cx))
                    })
                    .when(show_plan, |this| this.child(self.render_plan_ready(cx))),
            )
    }

    fn render_empty(&self, _: &mut Context<Self>) -> impl IntoElement + use<> {
        let mode = self.thread.mode;
        v_flex()
            .p_4()
            .gap_2()
            .items_center()
            .child(
                Icon::new(IconName::Sparkle)
                    .size(IconSize::Medium)
                    .color(Color::Muted),
            )
            .child(Label::new(format!("{} mode", mode.label())).size(LabelSize::Small))
            .child(
                Label::new(match (mode, self.advice) {
                    (Mode::Agent, false) => {
                        "Describe a change; the agent edits, you review each hunk."
                    }
                    (Mode::Ask, false) => "Ask about the code. Nothing will be edited.",
                    (Mode::Plan, false) => {
                        "Describe a feature; you get a plan to review, then Build."
                    }
                    (Mode::Agent, true) => {
                        "Ask for help; the agent asks before it changes anything."
                    }
                    (Mode::Ask, true) => "Ask anything. Nothing will be changed.",
                    (Mode::Plan, true) => "Describe a goal; you get a plan to review, then Build.",
                })
                .size(LabelSize::XSmall)
                .color(Color::Muted),
            )
            .when(!self.mcp_status.is_empty(), |this| {
                let ok = self.mcp_status.iter().filter(|s| s.result.is_ok()).count();
                this.child(
                    Label::new(format!(
                        "MCP: {ok}/{} servers connected",
                        self.mcp_status.len()
                    ))
                    .size(LabelSize::XSmall)
                    .color(Color::Muted),
                )
            })
    }

    fn render_user(
        &self,
        message: &ThreadMessage,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = message.id;
        let has_checkpoint = message.checkpoint.is_some();
        let bubble =
            v_flex()
                .max_w(relative(0.85))
                .min_w_0()
                .gap_1()
                .px_3()
                .py_2()
                .rounded(px(12.))
                .bg(ferrum_ui::active_fill(cx))
                .child(
                    h_flex()
                        .items_start()
                        .gap_1()
                        .child(div().flex_1().min_w_0().text_sm().child(message.text()))
                        .when(has_checkpoint && !self.busy, |this| {
                            this.child(
                                IconButton::new(("chat-restore", id), IconName::Undo)
                                    .icon_size(IconSize::XSmall)
                                    .icon_color(Color::Muted)
                                    .tooltip(Tooltip::text("Restore files to before this message"))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.run_blocking(cx, move |s| {
                                            s.restore_checkpoint(id).map(|_| ())
                                        });
                                    })),
                            )
                        }),
                )
                .when(!message.attachments.is_empty(), |this| {
                    this.child(h_flex().flex_wrap().gap_1().children(
                        message.attachments.iter().map(|a| {
                            div()
                                .px_1p5()
                                .rounded_sm()
                                .bg(ferrum_ui::hover_fill(cx))
                                .child(
                                    Label::new(a.label.clone())
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted)
                                        .single_line()
                                        .truncate(),
                                )
                        }),
                    ))
                });
        h_flex().w_full().justify_end().child(bubble)
    }

    /// Copy and retry under the last answer.
    fn render_turn_actions(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let answer = self
            .thread
            .messages
            .last()
            .map(|m| m.text())
            .unwrap_or_default();
        let prompt = self
            .thread
            .messages
            .iter()
            .rev()
            .find(|m| m.role == Role::User)
            .map(|m| m.text());
        h_flex()
            .px_1()
            .mt(px(-8.))
            .gap_0p5()
            .when(!answer.is_empty(), |this| {
                this.child(
                    ferrum_ui::title_button("chat-copy-answer", IconName::Copy, "Copy answer")
                        .icon_size(IconSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            cx.write_to_clipboard(gpui::ClipboardItem::new_string(answer.clone()));
                            this.show_notice(ferrum_ui::Notice::info("Copied."), cx);
                        })),
                )
            })
            .when_some(prompt, |this, prompt| {
                this.child(
                    ferrum_ui::title_button("chat-retry", IconName::RotateCw, "Send again")
                        .icon_size(IconSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| {
                            if let Some(session) = &this.session {
                                session.send(&prompt, Vec::new());
                                this.follow = true;
                            }
                            cx.notify();
                        })),
                )
            })
    }

    fn render_assistant(
        &mut self,
        message: &ThreadMessage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let id = message.id;
        v_flex()
            .w_full()
            .min_w_0()
            .gap_2()
            .px_1()
            .children(message.blocks.iter().enumerate().map(|(i, block)| {
                match block {
                    Block::Text { text } => markdown::render(format!("chat-md-{id}-{i}"), text, cx),
                    Block::Thinking { text } => {
                        self.render_thinking(id, i, text, cx).into_any_element()
                    }
                    Block::Tool { call } => self.render_tool(call, window, cx).into_any_element(),
                    Block::Plan { entries } => render_plan_entries(entries, cx).into_any_element(),
                    Block::Error { message } => div()
                        .p_2()
                        .rounded_md()
                        .border_1()
                        .border_color(cx.theme().status().error_border)
                        .bg(cx.theme().status().error_background.opacity(0.15))
                        .child(
                            Label::new(message.clone())
                                .size(LabelSize::Small)
                                .color(Color::Error),
                        )
                        .into_any_element(),
                }
            }))
    }

    fn render_thinking(
        &self,
        id: u64,
        i: usize,
        text: &str,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let key = (id, i);
        let open = self.expanded_thinking.contains(&key);
        v_flex()
            .gap_1()
            .child(
                Button::new(
                    ("chat-thinking", id * 1000 + i as u64),
                    if open { "Thinking ▾" } else { "Thinking ▸" },
                )
                .style(ButtonStyle::Transparent)
                .label_size(LabelSize::XSmall)
                .color(Color::Muted)
                .on_click(cx.listener(move |this, _, _, cx| {
                    if !this.expanded_thinking.remove(&key) {
                        this.expanded_thinking.insert(key);
                    }
                    cx.notify();
                })),
            )
            .when(open, |this| {
                this.child(
                    div()
                        .pl_2()
                        .border_l_1()
                        .border_color(cx.theme().colors().border_variant)
                        .text_xs()
                        .text_color(cx.theme().colors().text_muted)
                        .child(text.to_string()),
                )
            })
    }

    fn render_plan_ready(&self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let plan = self.thread.plan.clone().unwrap_or_default();
        v_flex()
            .p_2()
            .gap_2()
            .rounded_md()
            .border_1()
            .border_color(colors.border_focused)
            .child(
                h_flex()
                    .justify_between()
                    .child(Label::new("Plan").size(LabelSize::Small))
                    .child(
                        Button::new("chat-build", "Build")
                            .style(ButtonStyle::Filled)
                            .start_icon(Icon::new(IconName::Hammer).size(IconSize::XSmall))
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.build_plan(&BuildPlan, window, cx)
                            })),
                    ),
            )
            .child(markdown::render("chat-plan", &plan, cx))
    }
}

fn render_plan_entries(entries: &[PlanEntry], cx: &App) -> impl IntoElement + use<> {
    v_flex()
        .p_2()
        .gap_1()
        .rounded_md()
        .border_1()
        .border_color(cx.theme().colors().border_variant)
        .child(
            Label::new("Plan")
                .size(LabelSize::XSmall)
                .color(Color::Muted),
        )
        .children(entries.iter().map(|e| {
            let (mark, color) = match e.status.as_str() {
                "completed" => ("☑", Color::Success),
                "in_progress" => ("◐", Color::Accent),
                _ => ("☐", Color::Muted),
            };
            h_flex()
                .gap_1p5()
                .items_start()
                .child(Label::new(mark).size(LabelSize::Small).color(color))
                .child(div().flex_1().min_w_0().text_sm().child(e.content.clone()))
        }))
}

//! The composer: open permission prompts, queued messages and the
//! @-mention picker above a rounded card with the context chips, the input
//! and a row of controls — add context (+), attach (📎), mode, approvals,
//! the model/agent picker, the microphone and send/stop. The footer line
//! below it is [`crate::footer`].

use ferrum_ui::TextInput;
use ferrum_ui::components::ContentButton;
use ferrum_ui::prelude::*;
use ferrumagent_core::context::{self, Mention};
use ferrumagent_core::permissions::Decision;
use ferrumagent_core::session::AgentChoice;
use gpui::{Anchor, Context, Entity, Window};

use crate::actions::*;
use crate::attach::{DICTATION_HINT, mic_glyph};
use crate::menus::{approval_icon, mode_icon};
use crate::view::{ChatView, Picker};

/// `12.3k`.
pub(crate) fn tokens(n: u64) -> String {
    match n {
        n if n >= 1_000_000 => format!("{:.1}M", n as f64 / 1e6),
        n if n >= 1_000 => format!("{:.1}k", n as f64 / 1e3),
        n => n.to_string(),
    }
}

/// The `@query` being typed at `cursor`: (byte offset of `@`, query).
pub(crate) fn mention_query(text: &str, cursor: usize) -> Option<(usize, String)> {
    let before = text.get(..cursor)?;
    let start = before.rfind(|c: char| c.is_whitespace()).map_or(0, |i| {
        i + before[i..].chars().next().map_or(1, char::len_utf8)
    });
    let token = &before[start..];
    let query = token.strip_prefix('@')?;
    Some((start, query.to_string()))
}

impl ChatView {
    pub(crate) fn composer_changed(
        &mut self,
        input: Entity<TextInput>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (text, cursor) = {
            let input = input.read(cx);
            (input.text().to_string(), input.cursor())
        };
        let Some((start, query)) = mention_query(&text, cursor) else {
            if self.picker.take().is_some() {
                cx.notify();
            }
            return;
        };
        let root = self.project.clone();
        let search = query.clone();
        let task = cx.spawn(async move |this, cx| {
            let items = cx
                .background_spawn(async move { context::suggest(&root, &search, 8) })
                .await;
            this.update(cx, |this, cx| {
                this.picker = Some(Picker {
                    start,
                    query,
                    items,
                    selected: 0,
                });
                cx.notify();
            })
            .ok();
        });
        self.picker_task(Some(task));
    }

    pub(crate) fn mention_next(&mut self, _: &MentionNext, _: &mut Window, cx: &mut Context<Self>) {
        match self.picker.as_mut().filter(|p| !p.items.is_empty()) {
            Some(picker) => {
                picker.selected = (picker.selected + 1) % picker.items.len();
                cx.notify();
            }
            None => cx.propagate(),
        }
    }

    pub(crate) fn mention_prev(&mut self, _: &MentionPrev, _: &mut Window, cx: &mut Context<Self>) {
        match self.picker.as_mut().filter(|p| !p.items.is_empty()) {
            Some(picker) => {
                picker.selected = picker
                    .selected
                    .checked_sub(1)
                    .unwrap_or(picker.items.len() - 1);
                cx.notify();
            }
            None => cx.propagate(),
        }
    }

    pub(crate) fn accept_mention(
        &mut self,
        _: &AcceptMention,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(picker) = self.picker.take().filter(|p| !p.items.is_empty()) else {
            cx.propagate();
            return;
        };
        let choice = picker.items[picker.selected.min(picker.items.len() - 1)]
            .mention
            .clone();
        self.pick_mention(picker.start, picker.query.len() + 1, choice, window, cx);
    }

    /// Replaces the typed `@query` with a context chip.
    fn pick_mention(
        &mut self,
        start: usize,
        len: usize,
        mention: Mention,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.composer.update(cx, |input, cx| {
            let text = input.text().to_string();
            let end = (start + len).min(text.len());
            let mut updated = String::with_capacity(text.len());
            updated.push_str(&text[..start]);
            updated.push_str(text[end..].trim_start_matches(' '));
            input.set_text(updated, cx);
            input.select_range(start..start, cx);
        });
        self.picker = None;
        self.add_mention(mention, cx);
        self.composer
            .update(cx, |input, cx| input.focus(window, cx));
    }

    pub(crate) fn context_window(&self) -> Option<u64> {
        self.thread
            .context_window
            .or_else(|| match AgentChoice::parse(&self.thread.agent) {
                Some(AgentChoice::Native { provider, .. }) if provider == "anthropic" => {
                    Some(200_000)
                }
                Some(AgentChoice::Native { provider, .. }) if provider == "openai" => Some(400_000),
                _ => None,
            })
    }

    /// The logo of the chosen model's maker or ACP agent, if it has one.
    fn agent_logo(&self) -> Option<ferrum_ui::AgentLogo> {
        crate::threads::agent_logo(&self.shown_agent())
    }

    fn agent_label(&self) -> String {
        let shown = self.shown_agent();
        if let Some((key, label)) = &self.agent_name
            && *key == shown
        {
            return label.clone();
        }
        match AgentChoice::parse(&shown) {
            Some(AgentChoice::Native { model, .. }) => model,
            Some(AgentChoice::Acp { agent }) => self
                .models
                .acp_agents()
                .iter()
                .find(|a| a.id == agent)
                .map_or(agent, |a| a.name.clone()),
            None => "Choose a model".into(),
        }
    }

    /// "Describe a task…" on an empty thread, else "Continue this session…".
    fn sync_placeholder(&mut self, cx: &mut Context<Self>) {
        let empty = self.thread.messages.is_empty();
        if self.placeholder_empty != Some(empty) {
            self.placeholder_empty = Some(empty);
            let text = if self.advice && empty {
                "Ask anything…"
            } else {
                placeholder(empty)
            };
            self.composer
                .update(cx, |input, cx| input.set_placeholder(text, cx));
        }
    }

    /// The rounded card: context chips, the input and the control row.
    fn render_card(&mut self, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let mode = self.thread.mode;
        let approval = self.approval();
        let can_send =
            !self.composer.read(cx).text().trim().is_empty() || !self.mentions.is_empty();
        let small_button = |id: &'static str, label: &'static str, icon: IconName| {
            Button::new(id, label)
                .style(ButtonStyle::Subtle)
                .label_size(LabelSize::Small)
                .color(Color::Muted)
                .start_icon(Icon::new(icon).size(IconSize::XSmall).color(Color::Muted))
                .end_icon(
                    Icon::new(IconName::ChevronDown)
                        .size(IconSize::XSmall)
                        .color(Color::Muted),
                )
        };
        let agent = h_flex()
            .gap_1()
            .max_w(px(180.))
            .when_some(self.agent_logo(), |this, logo| {
                this.child(ferrum_ui::agent_logo(logo, px(14.), cx))
            })
            .child(
                div().min_w_0().overflow_hidden().child(
                    Label::new(self.agent_label())
                        .size(LabelSize::Small)
                        .single_line()
                        .truncate(),
                ),
            )
            .child(
                Icon::new(IconName::ChevronDown)
                    .size(IconSize::XSmall)
                    .color(Color::Muted),
            );
        let send = div()
            .id(if self.busy { "chat-stop" } else { "chat-send" })
            .flex_none()
            .size(px(26.))
            .ml_0p5()
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .bg(if self.busy || can_send {
                colors.text
            } else {
                colors.text.opacity(0.3)
            })
            .child(
                Icon::new(if self.busy {
                    IconName::Stop
                } else {
                    IconName::ArrowUp
                })
                .size(IconSize::Small)
                .color(Color::Custom(colors.editor_background)),
            )
            .tooltip(Tooltip::text(if self.busy {
                "Stop (Esc)"
            } else {
                "Send (⏎)"
            }))
            .on_click(cx.listener(|this, _, window, cx| {
                if this.busy {
                    this.stop(&Stop, window, cx);
                } else {
                    this.send(&Send, window, cx);
                }
            }));
        v_flex()
            .rounded(px(12.))
            .border_1()
            .border_color(colors.border)
            .bg(colors.editor_background)
            .when(!self.mentions.is_empty(), |this| {
                this.child(h_flex().px_2().pt_2().gap_1().flex_wrap().children(
                    self.mentions.iter().enumerate().map(|(i, m)| {
                        h_flex()
                            .pl_1p5()
                            .gap_1()
                            .rounded(px(6.))
                            .bg(ferrum_ui::active_fill(cx))
                            .child(
                                Icon::new(mention_icon(m))
                                    .size(IconSize::XSmall)
                                    .color(Color::Muted),
                            )
                            .child(
                                Label::new(m.label())
                                    .size(LabelSize::XSmall)
                                    .single_line()
                                    .truncate(),
                            )
                            .child(
                                IconButton::new(("chat-chip-x", i), IconName::Close)
                                    .icon_size(IconSize::XSmall)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        if i < this.mentions.len() {
                                            this.mentions.remove(i);
                                        }
                                        cx.notify();
                                    })),
                            )
                    }),
                ))
            })
            .child(
                div()
                    .key_context(crate::COMPOSER_CONTEXT)
                    .px_3()
                    .pt_2()
                    .max_h(px(220.))
                    .child(self.composer.clone()),
            )
            .child(
                h_flex()
                    .px_1p5()
                    .pt_1()
                    .pb_1p5()
                    .gap_1()
                    .flex_wrap()
                    .justify_between()
                    .child(
                        h_flex()
                            .gap_0p5()
                            .child(
                                PopoverMenu::new("chat-add")
                                    .trigger(ferrum_ui::title_button(
                                        "chat-add-button",
                                        IconName::Plus,
                                        "Add context",
                                    ))
                                    .anchor(Anchor::BottomLeft)
                                    .menu(self.add_context_menu(cx)),
                            )
                            .child(
                                ferrum_ui::title_button(
                                    "chat-attach",
                                    IconName::Link,
                                    "Attach files",
                                )
                                .on_click(cx.listener(|this, _, _, cx| this.pick_paths(true, cx))),
                            )
                            .child(
                                div().debug_selector(|| "chat-mode".into()).child(
                                    PopoverMenu::new("chat-mode")
                                        .trigger(
                                            small_button(
                                                "chat-mode-button",
                                                mode.label(),
                                                mode_icon(mode),
                                            )
                                            .tooltip(Tooltip::text("Mode (⇧Tab cycles)")),
                                        )
                                        .anchor(Anchor::BottomLeft)
                                        .menu(self.mode_menu(cx)),
                                ),
                            )
                            .child(
                                div().debug_selector(|| "chat-approval".into()).child(
                                    PopoverMenu::new("chat-approval")
                                        .trigger(
                                            small_button(
                                                "chat-approval-button",
                                                approval.label(),
                                                approval_icon(approval),
                                            )
                                            .tooltip(Tooltip::text(approval.description())),
                                        )
                                        .anchor(Anchor::BottomLeft)
                                        .menu(self.approval_menu(cx)),
                                ),
                            ),
                    )
                    .child(
                        h_flex()
                            .ml_auto()
                            .gap_0p5()
                            .child(
                                div().debug_selector(|| "chat-agent".into()).child(
                                    PopoverMenu::new("chat-agent")
                                        .trigger(
                                            ContentButton::new("chat-agent-button", agent)
                                                .style(ButtonStyle::Subtle)
                                                .tooltip(Tooltip::text("Model or agent")),
                                        )
                                        .anchor(Anchor::BottomRight)
                                        .menu(self.agent_menu(cx)),
                                ),
                            )
                            .child(
                                ContentButton::new("chat-mic", mic_glyph(colors.text_muted))
                                    .style(ButtonStyle::Subtle)
                                    .tooltip(Tooltip::text(DICTATION_HINT))
                                    .on_click(
                                        cx.listener(|this, _, window, cx| this.dictate(window, cx)),
                                    ),
                            )
                            .child(send),
                    ),
            )
    }

    pub(crate) fn render_composer(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let visible_tools: Vec<String> = self
            .thread
            .messages
            .iter()
            .flat_map(|m| m.blocks.iter())
            .filter_map(|b| match b {
                ferrumagent_core::thread::Block::Tool { call } => Some(call.id.clone()),
                _ => None,
            })
            .collect();
        let orphans: Vec<_> = self
            .permissions
            .iter()
            .filter(|p| !visible_tools.contains(&p.tool_call_id))
            .cloned()
            .collect();
        self.sync_placeholder(cx);
        let card = self.render_card(cx);
        let footer = self.render_footer(cx);
        v_flex()
            .flex_none()
            .px_3()
            .pt_1()
            .gap_1()
            .children(orphans.into_iter().map(|request| {
                let id = request.id;
                v_flex()
                    .m_1()
                    .p_2()
                    .gap_1()
                    .rounded_md()
                    .border_1()
                    .border_color(colors.border_focused)
                    .child(Label::new(format!("Allow: {}", request.title)).size(LabelSize::Small))
                    .child(
                        h_flex()
                            .gap_1()
                            .flex_wrap()
                            .child(
                                Button::new(("chat-perm-allow", id), "Allow")
                                    .style(ButtonStyle::Filled)
                                    .label_size(LabelSize::XSmall)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.respond(id, Decision::AllowOnce, cx)
                                    })),
                            )
                            .child(
                                Button::new(("chat-perm-session", id), "Allow for session")
                                    .label_size(LabelSize::XSmall)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.respond(id, Decision::AllowSession, cx)
                                    })),
                            )
                            .child(
                                Button::new(("chat-perm-deny", id), "Deny")
                                    .style(ButtonStyle::Tinted(
                                        ferrum_ui::components::TintColor::Error,
                                    ))
                                    .label_size(LabelSize::XSmall)
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.respond(id, Decision::Deny, cx)
                                    })),
                            ),
                    )
            }))
            .when(!self.queue.is_empty(), |this| {
                this.child(
                    v_flex()
                        .px_2()
                        .pt_1()
                        .gap_0p5()
                        .child(
                            Label::new(format!("Queued ({})", self.queue.len()))
                                .size(LabelSize::XSmall)
                                .color(Color::Muted),
                        )
                        .children(self.queue.iter().enumerate().map(|(i, text)| {
                            h_flex()
                                .gap_1()
                                .child(
                                    div().flex_1().min_w_0().overflow_hidden().child(
                                        Label::new(
                                            text.lines().next().unwrap_or_default().to_string(),
                                        )
                                        .size(LabelSize::XSmall)
                                        .single_line()
                                        .truncate(),
                                    ),
                                )
                                .child(
                                    IconButton::new(("chat-unqueue", i), IconName::Close)
                                        .icon_size(IconSize::XSmall)
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            if let Some(session) = &this.session {
                                                session.remove_queued(i);
                                            }
                                            cx.notify();
                                        })),
                                )
                        })),
                )
            })
            .when_some(
                self.picker.as_ref().filter(|p| !p.items.is_empty()),
                |this, picker| {
                    let start = picker.start;
                    let len = picker.query.len() + 1;
                    this.child(
                        v_flex()
                            .m_1()
                            .p_0p5()
                            .rounded_md()
                            .border_1()
                            .border_color(colors.border)
                            .bg(colors.elevated_surface_background)
                            .children(picker.items.iter().enumerate().map(|(i, item)| {
                                let mention = item.mention.clone();
                                ListItem::new(("chat-mention", i))
                                    .rounded()
                                    .toggle_state(i == picker.selected)
                                    .start_slot(
                                        Icon::new(mention_icon(&item.mention))
                                            .size(IconSize::XSmall)
                                            .color(Color::Muted),
                                    )
                                    .child(
                                        Label::new(item.label.clone())
                                            .size(LabelSize::Small)
                                            .single_line()
                                            .truncate(),
                                    )
                                    .end_slot(
                                        Label::new(item.detail.clone())
                                            .size(LabelSize::XSmall)
                                            .color(Color::Muted),
                                    )
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.pick_mention(start, len, mention.clone(), window, cx)
                                    }))
                            })),
                    )
                },
            )
            .child(card)
            .child(footer)
    }
}

/// The composer's placeholder.
pub(crate) fn placeholder(empty_thread: bool) -> &'static str {
    if empty_thread {
        "Describe a task… (@ adds context)"
    } else {
        "Continue this session…"
    }
}

fn mention_icon(mention: &Mention) -> IconName {
    match mention {
        Mention::Folder { .. } => IconName::Folder,
        Mention::GitDiff => IconName::GitBranch,
        Mention::Url { .. } => IconName::Link,
        Mention::Terminal { .. } => IconName::Terminal,
        Mention::Selection { .. } => IconName::CursorIBeam,
        Mention::File { .. } => IconName::File,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mention_queries_and_token_labels() {
        assert_eq!(
            mention_query("look @src/ma", 12),
            Some((5, "src/ma".into()))
        );
        assert_eq!(mention_query("@", 1), Some((0, String::new())));
        assert_eq!(mention_query("mail a@b", 8), None);
        assert_eq!(mention_query("x @ab y", 7), None);
        assert_eq!(tokens(999), "999");
        assert_eq!(tokens(12_345), "12.3k");
        assert_eq!(tokens(2_000_000), "2.0M");
    }
}

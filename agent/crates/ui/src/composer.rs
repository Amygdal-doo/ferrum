//! The composer: message input with an @-mention picker and context
//! chips, queued messages, open permission prompts, and the footer with
//! the mode switch, agent/model selector, token meter and send/stop.

use ferrum_ui::TextInput;
use ferrum_ui::components::{ContextMenu, DropdownMenu, DropdownStyle, ProgressBar};
use ferrum_ui::prelude::*;
use ferrumagent_core::context::{self, Mention};
use ferrumagent_core::modes::Mode;
use ferrumagent_core::permissions::Decision;
use ferrumagent_core::session::AgentChoice;
use gpui::{Context, Entity, Window};

use crate::actions::*;
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

    fn context_window(&self) -> Option<u64> {
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
        match AgentChoice::parse(&self.thread.agent)? {
            AgentChoice::Native { model, .. } => ferrum_ui::AgentLogo::for_model(&model),
            AgentChoice::Acp { agent } => {
                ferrum_ui::AgentLogo::for_program(agent.split(['-', ' ']).next().unwrap_or(&agent))
            }
        }
    }

    fn agent_label(&self) -> String {
        match AgentChoice::parse(&self.thread.agent) {
            Some(AgentChoice::Native { model, .. }) => model,
            Some(AgentChoice::Acp { agent }) => self
                .session
                .as_ref()
                .and_then(|s| {
                    s.acp_agents()
                        .iter()
                        .find(|a| a.id == agent)
                        .map(|a| a.name.clone())
                })
                .unwrap_or(agent),
            None => "Choose a model".into(),
        }
    }

    /// The selector's menu, rebuilt only when its options or selection
    /// change (not on every streamed render).
    fn agent_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Entity<ContextMenu> {
        let options = self
            .session
            .as_ref()
            .map(|s| s.agent_options())
            .unwrap_or_default();
        let current = self.thread.agent.clone();
        let signature = format!(
            "{current}|{}",
            options
                .iter()
                .map(|o| o.choice.key())
                .collect::<Vec<_>>()
                .join(",")
        );
        if let Some((built_for, menu)) = &self.agent_menu
            && *built_for == signature
        {
            return menu.clone();
        }
        let this = cx.entity().downgrade();
        let menu = ContextMenu::build(window, cx, move |mut menu, _, _| {
            let mut group = String::new();
            for option in options {
                if option.group != group {
                    if !group.is_empty() {
                        menu = menu.separator();
                    }
                    group = option.group.clone();
                    menu = menu.header(group.clone());
                }
                let this = this.clone();
                let choice = option.choice.clone();
                menu = menu.toggleable_entry(
                    option.label.clone(),
                    option.choice.key() == current,
                    IconPosition::Start,
                    None,
                    move |_, cx| {
                        let choice = choice.clone();
                        this.update(cx, |view, cx| view.set_agent(choice, cx)).ok();
                    },
                );
            }
            menu.separator().entry("Models and API keys…", None, {
                let this = this.clone();
                move |window, cx| {
                    this.update(cx, |view, cx| view.open_settings(window, cx))
                        .ok();
                }
            })
        });
        self.agent_menu = Some((signature, menu.clone()));
        menu
    }

    pub(crate) fn render_composer(
        &mut self,
        window: &mut Window,
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
        let mode = self.thread.mode;
        let used = self.thread.context_tokens;
        let window_size = self.context_window();
        let menu = self.agent_menu(window, cx);
        v_flex()
            .flex_none()
            .border_t_1()
            .border_color(colors.border)
            .bg(colors.editor_background)
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
                                        Icon::new(match item.mention {
                                            Mention::Folder { .. } => IconName::Folder,
                                            Mention::GitDiff => IconName::GitBranch,
                                            Mention::Url { .. } => IconName::Link,
                                            _ => IconName::File,
                                        })
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
            .when(!self.mentions.is_empty(), |this| {
                this.child(h_flex().px_2().pt_1().gap_1().flex_wrap().children(
                    self.mentions.iter().enumerate().map(|(i, m)| {
                        h_flex()
                            .pl_1p5()
                            .rounded_sm()
                            .bg(colors.ghost_element_selected)
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
                    .px_2()
                    .pt_1()
                    .max_h(px(220.))
                    .child(self.composer.clone()),
            )
            .child(
                h_flex()
                    .px_1()
                    .pb_1()
                    .gap_1()
                    .flex_wrap()
                    .child(h_flex().gap_0p5().children(Mode::ALL.into_iter().map(|m| {
                        Button::new(
                            SharedString::from(format!("chat-mode-{}", m.label())),
                            m.label(),
                        )
                        .style(ButtonStyle::Subtle)
                        .label_size(LabelSize::XSmall)
                        .toggle_state(m == mode)
                        .tooltip(Tooltip::text("⇧Tab cycles modes"))
                        .on_click(cx.listener(move |this, _, _, cx| this.set_mode(m, cx)))
                    })))
                    .when_some(self.agent_logo(), |this, logo| {
                        this.child(div().ml_1().child(ferrum_ui::agent_logo(logo, px(14.), cx)))
                    })
                    .child(
                        DropdownMenu::new("chat-agent", self.agent_label(), menu)
                            .style(DropdownStyle::Subtle),
                    )
                    .child(div().flex_1())
                    .when(used > 0, |this| {
                        let label = match window_size {
                            Some(size) => format!("{} / {}", tokens(used), tokens(size)),
                            None => tokens(used),
                        };
                        this.child(
                            h_flex()
                                .gap_1()
                                .child(
                                    Label::new(label)
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted),
                                )
                                .when_some(window_size, |this, size| {
                                    this.child(div().w(px(40.)).child(ProgressBar::new(
                                        "chat-tokens",
                                        used as f32,
                                        size as f32,
                                        cx,
                                    )))
                                }),
                        )
                    })
                    .child(if self.busy {
                        IconButton::new("chat-stop", IconName::Stop)
                            .icon_size(IconSize::Small)
                            .icon_color(Color::Error)
                            .tooltip(Tooltip::text("Stop (Esc)"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.stop(&Stop, window, cx)),
                            )
                    } else {
                        IconButton::new("chat-send", IconName::ArrowUp)
                            .icon_size(IconSize::Small)
                            .tooltip(Tooltip::text("Send (⏎)"))
                            .on_click(
                                cx.listener(|this, _, window, cx| this.send(&Send, window, cx)),
                            )
                    }),
            )
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

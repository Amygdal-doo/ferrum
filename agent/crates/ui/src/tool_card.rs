//! Tool-call cards: status, title, permission prompt, output, diffs, and
//! "open file" / "run in terminal" hand-offs to the host.

use ferrum_ui::prelude::*;
use ferrum_ui::theme::theme_settings;
use ferrumagent_core::permissions::Decision;
use ferrumagent_core::thread::{ToolCallRecord, ToolStatus};
use gpui::{Context, Window};

use crate::view::{ChatEvent, ChatView};

const OUTPUT_PREVIEW: usize = 4000;

fn status_icon(status: ToolStatus) -> (IconName, Color) {
    match status {
        ToolStatus::Pending => (IconName::Circle, Color::Muted),
        ToolStatus::AwaitingPermission => (IconName::Lock, Color::Warning),
        ToolStatus::Running => (IconName::LoadCircle, Color::Accent),
        ToolStatus::Completed => (IconName::Check, Color::Success),
        ToolStatus::Failed => (IconName::XCircle, Color::Error),
        ToolStatus::Denied => (IconName::Close, Color::Error),
        ToolStatus::Cancelled => (IconName::Dash, Color::Muted),
    }
}

fn preview(text: &str) -> String {
    if text.len() <= OUTPUT_PREVIEW {
        return text.to_string();
    }
    let mut end = OUTPUT_PREVIEW;
    while !text.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}\n…", &text[..end])
}

impl ChatView {
    pub(crate) fn render_tool(
        &self,
        call: &ToolCallRecord,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let mono = theme_settings(cx).buffer_font.family.clone();
        let key = call.id.clone();
        let open = self.expanded_tools.contains(&call.id);
        let (icon, color) = status_icon(call.status);
        let request = self
            .permissions
            .iter()
            .find(|p| p.tool_call_id == call.id)
            .cloned();
        let id_base = SharedString::from(format!("tool-{}", call.id));
        let has_details = call.output.is_some() || !call.diffs.is_empty();
        let command = (call.name == "run_command")
            .then(|| call.input["command"].as_str().map(str::to_string))
            .flatten();
        v_flex()
            .w_full()
            .rounded_md()
            .border_1()
            .border_color(if request.is_some() {
                colors.border_focused
            } else {
                colors.border_variant
            })
            .bg(colors.surface_background)
            .child(
                h_flex()
                    .id(SharedString::from(format!("{id_base}-head")))
                    .px_2()
                    .py_1()
                    .gap_1p5()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, _, cx| {
                        if !this.expanded_tools.remove(&key) {
                            this.expanded_tools.insert(key.clone());
                        }
                        cx.notify();
                    }))
                    .child(Icon::new(icon).size(IconSize::XSmall).color(color))
                    .child(
                        div().flex_1().min_w_0().overflow_hidden().child(
                            Label::new(call.title.clone())
                                .size(LabelSize::Small)
                                .single_line()
                                .truncate(),
                        ),
                    )
                    .when_some(command.clone(), |this, cmd| {
                        let cwd = self.project.clone();
                        this.child(
                            IconButton::new(
                                SharedString::from(format!("{id_base}-term")),
                                IconName::Terminal,
                            )
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Run in terminal"))
                            .on_click(cx.listener(
                                move |_, _, _, cx| {
                                    cx.emit(ChatEvent::RunInTerminal {
                                        cmd: cmd.clone(),
                                        cwd: cwd.clone(),
                                    });
                                },
                            )),
                        )
                    })
                    .when(has_details, |this| {
                        this.child(
                            Icon::new(if open {
                                IconName::ChevronDown
                            } else {
                                IconName::ChevronRight
                            })
                            .size(IconSize::XSmall)
                            .color(Color::Muted),
                        )
                    }),
            )
            .when(!call.changed.is_empty(), |this| {
                this.child(h_flex().px_2().pb_1().flex_wrap().gap_1().children(
                    call.changed.iter().enumerate().map(|(i, path)| {
                        let rel = path.clone();
                        Button::new(
                            SharedString::from(format!("{id_base}-file-{i}")),
                            path.to_string_lossy().to_string(),
                        )
                        .style(ButtonStyle::Transparent)
                        .label_size(LabelSize::XSmall)
                        .color(Color::Accent)
                        .on_click(cx.listener(move |this, _, _, cx| this.open_file(&rel, None, cx)))
                    }),
                ))
            })
            .when_some(request, |this, request| {
                let id = request.id;
                let button = |label: &'static str,
                              suffix: &str,
                              decision: Decision,
                              style: ButtonStyle,
                              cx: &mut Context<Self>| {
                    Button::new(SharedString::from(format!("{id_base}-{suffix}")), label)
                        .style(style)
                        .label_size(LabelSize::XSmall)
                        .on_click(cx.listener(move |this, _, _, cx| this.respond(id, decision, cx)))
                };
                this.child(
                    v_flex()
                        .px_2()
                        .pb_2()
                        .gap_1()
                        .child(
                            Label::new("Allow this?")
                                .size(LabelSize::XSmall)
                                .color(Color::Warning),
                        )
                        .child(
                            h_flex()
                                .flex_wrap()
                                .gap_1()
                                .child(button(
                                    "Allow",
                                    "allow",
                                    Decision::AllowOnce,
                                    ButtonStyle::Filled,
                                    cx,
                                ))
                                .child(button(
                                    "Allow for session",
                                    "session",
                                    Decision::AllowSession,
                                    ButtonStyle::Outlined,
                                    cx,
                                ))
                                .child(button(
                                    "Always",
                                    "always",
                                    Decision::AllowAlways,
                                    ButtonStyle::Outlined,
                                    cx,
                                ))
                                .child(button(
                                    "Deny",
                                    "deny",
                                    Decision::Deny,
                                    ButtonStyle::Tinted(ferrum_ui::components::TintColor::Error),
                                    cx,
                                )),
                        ),
                )
            })
            .when(open && has_details, |this| {
                this.child(
                    v_flex()
                        .border_t_1()
                        .border_color(colors.border_variant)
                        .children(call.diffs.iter().map(|d| {
                            v_flex()
                                .px_2()
                                .py_1()
                                .font_family(mono.clone())
                                .text_xs()
                                .child(
                                    Label::new(d.path.to_string_lossy().to_string())
                                        .size(LabelSize::XSmall)
                                        .color(Color::Muted),
                                )
                                .children(d.old_text.iter().flat_map(|t| t.lines().take(40)).map(
                                    |l| {
                                        div()
                                            .text_color(cx.theme().status().deleted)
                                            .child(format!("- {l}"))
                                    },
                                ))
                                .children(d.new_text.lines().take(80).map(|l| {
                                    div()
                                        .text_color(cx.theme().status().created)
                                        .child(format!("+ {l}"))
                                }))
                        }))
                        .when_some(call.output.clone(), |this, output| {
                            this.child(
                                div()
                                    .id(SharedString::from(format!("{id_base}-out")))
                                    .max_h(px(240.))
                                    .overflow_y_scroll()
                                    .px_2()
                                    .py_1()
                                    .font_family(mono.clone())
                                    .text_xs()
                                    .when(call.is_error, |d| {
                                        d.text_color(cx.theme().status().error)
                                    })
                                    .child(preview(&output)),
                            )
                        }),
                )
            })
    }
}

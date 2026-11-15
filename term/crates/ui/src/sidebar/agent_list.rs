//! The sidebar's Agents section: every coding agent in any terminal,
//! grouped Needs you → Done → Working → Idle with a count per group. A row
//! shows the agent's logo, its tab name, a muted subtitle (its title or
//! directory), the time since its status last changed and a status mark;
//! clicking it shows that terminal.

use super::model::{AgentGroup, Model, Row, relative_time};
use crate::view::TermView;
use ferrum_ui::components::{
    Color, Icon, IconName, IconSize, StyledTypography, Tooltip, h_flex, v_flex,
};
use ferrum_ui::gpui::{
    self, Animation, AnimationExt as _, AnyElement, Context, Hsla, SharedString, Transformation,
    div, percentage, prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;
use ferrumterm_protocol::ProgramState;
use std::time::Duration;

impl TermView {
    pub(crate) fn render_agents(&self, model: &Model, cx: &mut Context<Self>) -> impl IntoElement {
        let mut section = v_flex().child(
            h_flex()
                .pr_2()
                .gap_1()
                .child(ferrum_ui::sidebar_label("Agents", cx))
                .when(model.attention > 0, |header| {
                    header.child(ferrum_ui::badge(
                        model.attention.to_string(),
                        cx.theme().status().warning,
                        cx,
                    ))
                })
                .child(div().flex_1())
                .child(self.new_menu("agents-new-menu", cx)),
        );
        if model.groups.is_empty() {
            section = section.child(super::placeholder("No agents yet. Start one with +", cx));
        }
        let now = now_ms();
        for (group, rows) in &model.groups {
            section = section.child(group_header(*group, rows.len(), cx));
            for row in rows {
                section = section.child(self.agent_row(row, now, cx));
            }
        }
        section
    }

    fn agent_row(&self, row: &Row, now: u64, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let (session, block) = (row.session, row.block);
        let mark = match row.logo {
            Some(logo) => ferrum_ui::agent_logo(logo, px(16.), cx).into_any_element(),
            None => Icon::new(IconName::Sparkle)
                .size(IconSize::Small)
                .color(Color::Muted)
                .into_any_element(),
        };
        let since = row.changed_ms.map(|ms| relative_time(now, ms));
        let tooltip = row
            .status
            .as_ref()
            .and_then(|s| s.message.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| row.detail.clone());
        h_flex()
            .id(SharedString::from(format!("agent-{}", row.block)))
            .mx_1p5()
            .px_2()
            .py(px(4.))
            .gap_2()
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|style| style.bg(ferrum_ui::hover_fill(cx)))
            .when(row.active, |r| r.bg(ferrum_ui::active_fill(cx)))
            .when(!tooltip.is_empty(), |r| r.tooltip(Tooltip::text(tooltip)))
            .on_click(cx.listener(move |view, _, window, cx| {
                window.focus(&view.focus_handle, cx);
                view.app.focus_block(session, block);
                view.close_overlay_sidebar();
                view.after_change(cx);
            }))
            .child(mark)
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .truncate()
                            .when(row.attention, |d| d.font_weight(gpui::FontWeight::SEMIBOLD))
                            .child(row.title.clone()),
                    )
                    .when(!row.subtitle.is_empty(), |col| {
                        col.child(
                            div()
                                .truncate()
                                .text_size(px(11.))
                                .text_color(colors.text_muted)
                                .child(row.subtitle.clone()),
                        )
                    }),
            )
            .when_some(since, |line, since| {
                line.child(
                    div()
                        .flex_none()
                        .text_size(px(11.))
                        .text_color(colors.text_muted)
                        .font_buffer(cx)
                        .child(since),
                )
            })
            .child(status_mark(row, cx))
    }
}

fn group_header(group: AgentGroup, count: usize, cx: &gpui::App) -> impl IntoElement {
    let colors = cx.theme().colors();
    h_flex()
        .px_3()
        .pt_1p5()
        .pb_0p5()
        .gap_1p5()
        .text_size(px(11.))
        .text_color(colors.text_muted)
        .child(group.title())
        .child(
            div()
                .text_color(colors.text_placeholder)
                .child(count.to_string()),
        )
}

/// ⚠ Needs you, a blue dot for Done (red when it failed), a spinner while
/// Working, a hollow circle when Idle.
fn status_mark(row: &Row, cx: &gpui::App) -> AnyElement {
    let dot = |color: Hsla| div().flex_none().size(px(7.)).rounded_full().bg(color);
    let failed = row
        .status
        .as_ref()
        .is_some_and(|s| s.state == ProgramState::Error);
    let mark = match row.group {
        AgentGroup::NeedsYou => Icon::new(IconName::Warning)
            .size(IconSize::XSmall)
            .color(Color::Warning)
            .into_any_element(),
        AgentGroup::Done if failed => dot(cx.theme().status().error).into_any_element(),
        AgentGroup::Done => dot(cx.theme().colors().text_accent).into_any_element(),
        AgentGroup::Working => Icon::new(IconName::LoadCircle)
            .size(IconSize::XSmall)
            .color(Color::Accent)
            .with_animation(
                SharedString::from(format!("agent-working-{}", row.block)),
                Animation::new(Duration::from_secs(1)).repeat(),
                |icon, delta| icon.transform(Transformation::rotate(percentage(delta))),
            )
            .into_any_element(),
        AgentGroup::Idle => div()
            .flex_none()
            .size(px(7.))
            .rounded_full()
            .border_1()
            .border_color(cx.theme().colors().text_muted)
            .into_any_element(),
    };
    div()
        .flex_none()
        .w(px(14.))
        .flex()
        .justify_center()
        .child(mark)
        .into_any_element()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

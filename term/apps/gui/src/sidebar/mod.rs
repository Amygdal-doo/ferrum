//! The collapsible left sidebar (⌘B): an "Agents" section with status dots
//! and a badge for agents that need you, every session's tabs and panes, and
//! a "New agent" menu (Claude Code, Codex, recent or custom commands), and
//! SSH hosts with their port forwards (see `hosts`).

pub mod agents;
mod hosts;
pub mod model;

use crate::view::TermView;
use ferrum_ui::gpui::{
    self, Context, DragMoveEvent, Hsla, SharedString, Window, div, prelude::*, px,
};
use ferrum_ui::theme::ActiveTheme;
use ferrum_ui::ui::{
    ButtonCommon, Clickable, ContextMenu, IconButton, IconName, IconSize, PopoverMenu,
    StyledTypography, Tooltip, h_flex, v_flex,
};
use ferrum_ui::{DraggedSplitter, TextInput, TextInputEvent, badge};
use ferrumterm_protocol::{ProgramState, ProgramStatus};
use model::{Model, Row};

impl TermView {
    pub(crate) fn render_sidebar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let model = Model::build(&self.app.sessions, self.app.window(), &self.app.seen);
        let mut list = v_flex()
            .id("sidebar-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pb_2();
        if model.agents.is_empty() {
            list = list.child(
                div()
                    .px_3()
                    .py_1()
                    .text_size(px(12.))
                    .text_color(colors.text_placeholder)
                    .child("No agents yet. Start one with +"),
            );
        }
        for row in &model.agents {
            list = list.child(self.render_row(format!("agent-{}", row.block), row, cx));
        }
        for section in &model.sessions {
            list = list.child(heading(section.title.clone(), cx).mt_2());
            for row in &section.rows {
                list = list.child(self.render_row(format!("row-{}", row.block), row, cx));
            }
        }
        list = list.child(self.render_hosts(cx));

        v_flex()
            .flex_none()
            .w(px(self.app.ui.sidebar_width))
            .h_full()
            .bg(colors.panel_background)
            .font_ui(cx)
            .text_size(px(13.))
            .text_color(colors.text)
            .child(
                h_flex()
                    .flex_none()
                    .h(px(34.))
                    .pl_3()
                    .pr_1()
                    .gap_1p5()
                    .border_b_1()
                    .border_color(colors.border)
                    .child(heading("Agents", cx).px_0())
                    .when(model.attention > 0, |header| {
                        header.child(badge(
                            model.attention.to_string(),
                            cx.theme().status().warning,
                            cx,
                        ))
                    })
                    .child(div().flex_1())
                    .child(self.new_agent_menu(cx)),
            )
            .when_some(self.custom_agent.clone(), |sidebar, input| {
                sidebar.child(div().flex_none().p_2().child(input))
            })
            .child(list)
    }

    fn render_row(&self, id: String, row: &Row, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors().clone();
        let dot = row.status.as_ref().map(|s| state_color(s.state, cx));
        let (session, block) = (row.session, row.block);
        let second = row
            .status
            .as_ref()
            .and_then(|s| s.message.clone())
            .filter(|m| !m.is_empty())
            .unwrap_or_else(|| row.detail.clone());
        v_flex()
            .id(SharedString::from(id))
            .px_3()
            .py(px(3.))
            .when(row.nested, |r| r.pl(px(26.)))
            .cursor_pointer()
            .hover(|style| style.bg(colors.ghost_element_hover))
            .when(row.active, |r| r.bg(colors.ghost_element_selected))
            .on_click(cx.listener(move |view, _, window, cx| {
                window.focus(&view.focus_handle, cx);
                view.app.focus_block(session, block);
                view.after_change(cx);
            }))
            .child(
                h_flex()
                    .gap_1p5()
                    .min_w_0()
                    .when_some(dot, |line, color| {
                        line.child(div().flex_none().size(px(7.)).rounded_full().bg(color))
                    })
                    .child(div().flex_1().min_w_0().truncate().child(row.title.clone()))
                    .when_some(row.status.as_ref(), |line, status| {
                        line.child(
                            div()
                                .flex_none()
                                .text_size(px(11.))
                                .text_color(state_color(status.state, cx))
                                .when(row.attention, |d| d.font_weight(gpui::FontWeight::BOLD))
                                .child(state_text(status)),
                        )
                    }),
            )
            .when(!second.is_empty(), |r| {
                r.child(
                    div()
                        .min_w_0()
                        .truncate()
                        .text_size(px(11.))
                        .text_color(colors.text_muted)
                        .child(second),
                )
            })
    }

    fn new_agent_menu(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let weak = cx.weak_entity();
        let recent = self.app.ui.recent_agents.clone();
        PopoverMenu::new("new-agent")
            .anchor(gpui::Anchor::TopRight)
            .trigger(
                IconButton::new("new-agent-button", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("New agent in a new tab")),
            )
            .menu(move |window, cx| {
                let weak = weak.clone();
                let recent = recent.clone();
                Some(ContextMenu::build(window, cx, move |mut menu, _, _| {
                    menu = menu.header("Start in a new tab");
                    let launch = |line: String, remember: bool| {
                        let weak = weak.clone();
                        move |window: &mut Window, cx: &mut gpui::App| {
                            if let Err(error) = weak.update(cx, |view, cx| {
                                window.focus(&view.focus_handle, cx);
                                view.launch_agent(&line, remember, cx);
                            }) {
                                tracing::warn!(%error, "sidebar closed before the agent started");
                            }
                        }
                    };
                    for (title, line) in agents::PRESETS {
                        menu = menu.entry(title, None, launch(line.to_owned(), false));
                    }
                    if !recent.is_empty() {
                        menu = menu.separator();
                        for line in &recent {
                            menu = menu.entry(line.clone(), None, launch(line.clone(), true));
                        }
                    }
                    let weak = weak.clone();
                    menu.separator()
                        .entry("Custom command…", None, move |window, cx| {
                            if let Err(error) =
                                weak.update(cx, |view, cx| view.begin_custom_agent(window, cx))
                            {
                                tracing::warn!(%error, "sidebar closed before the prompt opened");
                            }
                        })
                }))
            })
    }

    fn launch_agent(&mut self, command_line: &str, remember: bool, cx: &mut Context<Self>) {
        self.app.start_agent(command_line);
        if remember {
            self.app.ui.remember_agent(command_line);
            self.app.ui.save();
        }
        self.after_change(cx);
    }

    /// Opens the inline "custom command" field: Enter starts it, Escape or
    /// clicking elsewhere cancels.
    fn begin_custom_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.app.ui.sidebar_open {
            self.app.toggle_sidebar();
        }
        let input = cx.new(|cx| {
            TextInput::new(window, cx)
                .placeholder("Command, e.g. aider --model sonnet")
                .monospace()
        });
        let subscription =
            cx.subscribe_in(
                &input,
                window,
                |view, input, event, window, cx| match event {
                    TextInputEvent::Submit => {
                        let line = input.read(cx).text().trim().to_owned();
                        view.end_custom_agent(window, cx);
                        if !line.is_empty() {
                            view.launch_agent(&line, true, cx);
                        }
                    }
                    TextInputEvent::Blurred => view.end_custom_agent(window, cx),
                    TextInputEvent::Changed => {}
                },
            );
        input.update(cx, |input, cx| input.focus(window, cx));
        self.custom_agent = Some(input);
        self._custom_agent_events = Some(subscription);
        cx.notify();
    }

    pub(crate) fn end_custom_agent(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.custom_agent.take().is_some() {
            self._custom_agent_events = None;
            window.focus(&self.focus_handle, cx);
            cx.notify();
        }
    }

    /// Drag of the splitter between the sidebar and the terminal.
    pub(crate) fn resize_sidebar(
        &mut self,
        event: &DragMoveEvent<DraggedSplitter>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.drag(cx).id.as_ref() != "sidebar" {
            return;
        }
        let x = f32::from(event.event.position.x - event.bounds.left());
        self.app.ui.sidebar_width =
            x.clamp(crate::ui_state::MIN_SIDEBAR, crate::ui_state::MAX_SIDEBAR);
        self.resizing_sidebar = true;
        cx.notify();
    }

    /// Tab-bar button that shows / hides the sidebar, with the attention
    /// badge when the sidebar is hidden.
    pub(crate) fn sidebar_toggle(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let open = self.app.ui.sidebar_open;
        let attention = if open {
            0
        } else {
            Model::build(&self.app.sessions, self.app.window(), &self.app.seen).attention
        };
        h_flex()
            .flex_none()
            .gap_1()
            .mr(px(4.))
            .child(
                IconButton::new(
                    "toggle-sidebar",
                    if open {
                        IconName::ThreadsSidebarLeftOpen
                    } else {
                        IconName::ThreadsSidebarLeftClosed
                    },
                )
                .icon_size(IconSize::Small)
                .tooltip(Tooltip::text(if open {
                    "Hide sidebar (⌘B)"
                } else {
                    "Show sidebar (⌘B)"
                }))
                .on_click(cx.listener(|view, _, _, cx| {
                    view.app.toggle_sidebar();
                    view.after_change(cx);
                })),
            )
            .when(attention > 0, |toggle| {
                toggle.child(badge(
                    attention.to_string(),
                    cx.theme().status().warning,
                    cx,
                ))
            })
    }
}

fn heading(title: impl Into<SharedString>, cx: &gpui::App) -> gpui::Div {
    div()
        .px_3()
        .py_1()
        .text_size(px(11.))
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_color(cx.theme().colors().text_muted)
        .child(title.into().to_uppercase())
}

fn state_color(state: ProgramState, cx: &gpui::App) -> Hsla {
    let status = cx.theme().status();
    match state {
        ProgramState::Working => status.info,
        ProgramState::Blocked => status.warning,
        ProgramState::Error => status.error,
        ProgramState::Done => status.success,
        ProgramState::Idle => cx.theme().colors().text_muted,
    }
}

/// `working 40%`, `blocked · permission`, `done`…
fn state_text(status: &ProgramStatus) -> String {
    let mut text = status.state.as_str().to_owned();
    if let Some(p) = status.progress {
        text.push_str(&format!(" {p}%"));
    }
    if let Some(kind) = status.kind.as_ref().filter(|k| !k.is_empty()) {
        text.push_str(&format!(" · {kind}"));
    }
    text
}

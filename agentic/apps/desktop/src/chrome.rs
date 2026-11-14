//! The window: the native title bar is hidden; a title row (traffic
//! lights, ⌘ project switcher, + new chat, the project, the column
//! toggles), then the columns as cards, and notices.

use ferrum_ui::components::{Clickable, Toggleable};
use ferrum_ui::prelude::*;

use crate::app::{FerrumAgentic, KEY_CONTEXT, ToggleChat, ToggleGit, ToggleTerm};
use crate::columns::{icon, id};
use crate::layout::Column;

impl FerrumAgentic {
    pub(crate) fn title_row(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let colors = cx.theme().colors();
        let toggles = Column::ALL.into_iter().enumerate().map(|(i, column)| {
            let visible = self.layout.is_visible(column);
            let verb = if visible { "Hide" } else { "Show" };
            ferrum_ui::title_button(
                id("toggle", column),
                icon(column),
                format!("{verb} {} (⌥⌘{})", column.title(), i + 1),
            )
            .toggle_state(visible)
            .when(visible, |b| b.icon_color(Color::Accent))
            .on_click(cx.listener(move |this, _, window, cx| {
                this.on_toggle(column, window, cx);
            }))
        });
        ferrum_ui::title_row("title-row", true, window)
            .gap_1()
            .child(div().w(px(4.)))
            .child(
                ferrum_ui::command_button("switch-project", "Switch project (⌥⌘O)").on_click(
                    cx.listener(|this, _, window, cx| {
                        this.project_bar
                            .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
                    }),
                ),
            )
            .child(
                ferrum_ui::title_button("new-chat", IconName::Plus, "New chat (⌥⌘N)")
                    .on_click(cx.listener(|this, _, window, cx| this.new_chat(window, cx))),
            )
            .child(div().w(px(6.)))
            .child(
                div()
                    .flex_none()
                    .max_w(px(420.))
                    .child(self.project_bar.clone()),
            )
            .child(div().flex_1())
            // Sets the project's "open in" buttons apart from the toggles.
            .child(
                div()
                    .flex_none()
                    .w(px(1.))
                    .h(px(16.))
                    .mx_1()
                    .bg(colors.border_variant),
            )
            .children(toggles)
    }
}

impl Render for FerrumAgentic {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.follow_focus(window, cx);
        let colors = cx.theme().colors();
        let notice = self.notice.as_ref().map(|notice| {
            render_notice(
                notice,
                cx.listener(|this, _, _, cx| {
                    this.notice = None;
                    cx.notify();
                }),
                cx,
            )
        });
        v_flex()
            .id("ferrumagentic")
            .key_context(KEY_CONTEXT)
            .track_focus(self.focus_handle())
            .on_action(cx.listener(|this, _: &ToggleChat, window, cx| {
                this.on_toggle(Column::Chat, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleGit, window, cx| {
                this.on_toggle(Column::Git, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleTerm, window, cx| {
                this.on_toggle(Column::Term, window, cx);
            }))
            .on_action(cx.listener(|this, _: &ToggleProjectSwitcher, window, cx| {
                this.project_bar
                    .update(cx, |bar, cx| bar.toggle_switcher(window, cx));
            }))
            .relative()
            .size_full()
            .text_color(colors.text)
            .child(self.title_row(window, cx))
            .child(self.columns(cx))
            .children(notice)
    }
}

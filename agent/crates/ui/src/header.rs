//! The header: thread list toggle, title, changes and settings buttons.

use ferrum_ui::prelude::*;
use gpui::{Context, Window};

use crate::actions::*;
use crate::view::ChatView;

impl ChatView {
    pub(crate) fn render_header(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let colors = cx.theme().colors().clone();
        let changes = self.changes.len();
        h_flex()
            .flex_none()
            .h(px(36.))
            .px_1()
            .gap_1()
            .border_b_1()
            .border_color(colors.border)
            .bg(colors.toolbar_background)
            .child(
                IconButton::new("chat-threads", IconName::ThreadsSidebarLeftOpen)
                    .icon_size(IconSize::Small)
                    .toggle_state(self.show_threads)
                    .tooltip(Tooltip::text("Threads (⌘⌥J)"))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_threads(&ToggleThreads, window, cx)
                    })),
            )
            .child(
                div().flex_1().min_w_0().overflow_hidden().child(
                    Label::new(self.thread.title.clone())
                        .size(LabelSize::Small)
                        .single_line()
                        .truncate(),
                ),
            )
            .when(changes > 0, |this| {
                this.child(
                    Button::new("chat-changes", format!("{changes} changed"))
                        .label_size(LabelSize::XSmall)
                        .style(ButtonStyle::Subtle)
                        .color(Color::Modified)
                        .toggle_state(self.show_changes)
                        .tooltip(Tooltip::text("Review changes (⌘⌥R)"))
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.toggle_changes(&ToggleChanges, window, cx)
                        })),
                )
            })
            .child(
                IconButton::new("chat-new", IconName::Plus)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("New thread (⌘⌥N)"))
                    .on_click(
                        cx.listener(|this, _, window, cx| this.new_thread(&NewThread, window, cx)),
                    ),
            )
            .child(
                IconButton::new("chat-settings", IconName::Settings)
                    .icon_size(IconSize::Small)
                    .tooltip(Tooltip::text("Models and API keys"))
                    .on_click(cx.listener(|this, _, window, cx| this.open_settings(window, cx))),
            )
    }
}

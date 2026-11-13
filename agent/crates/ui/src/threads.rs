//! The thread history list (replaces the chat column while open).

use ferrum_ui::prelude::*;
use gpui::{Context, Window};

use crate::view::ChatView;

/// "5m ago" for a number of seconds.
pub(crate) fn ago(seconds: i64) -> String {
    match seconds {
        s if s < 60 => "just now".into(),
        s if s < 3600 => format!("{}m ago", s / 60),
        s if s < 86_400 => format!("{}h ago", s / 3600),
        s => format!("{}d ago", s / 86_400),
    }
}

impl ChatView {
    pub(crate) fn render_threads(
        &mut self,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_secs() as i64);
        let current = self.thread.id.clone();
        v_flex()
            .id("chat-thread-list")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_1()
            .gap_0p5()
            .child(
                h_flex()
                    .px_2()
                    .py_1()
                    .justify_between()
                    .child(
                        Label::new("Threads")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    )
                    .child(
                        Label::new(format!("{}", self.threads.len()))
                            .size(LabelSize::XSmall)
                            .color(Color::Muted),
                    ),
            )
            .when(self.threads.is_empty(), |this| {
                this.child(
                    div().p_4().child(
                        Label::new("No saved threads for this project yet.")
                            .size(LabelSize::Small)
                            .color(Color::Muted),
                    ),
                )
            })
            .children(self.threads.iter().enumerate().map(|(i, t)| {
                let id = t.id.clone();
                let delete_id = t.id.clone();
                let detail = format!(
                    "{} · {} messages · {}",
                    t.agent,
                    t.messages,
                    ago(now - t.updated.timestamp())
                );
                ListItem::new(("chat-thread", i))
                    .rounded()
                    .toggle_state(t.id == current)
                    .child(
                        v_flex()
                            .min_w_0()
                            .child(
                                Label::new(t.title.clone())
                                    .size(LabelSize::Small)
                                    .single_line()
                                    .truncate(),
                            )
                            .child(
                                Label::new(detail)
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted)
                                    .single_line()
                                    .truncate(),
                            ),
                    )
                    .end_slot(
                        IconButton::new(("chat-thread-delete", i), IconName::Trash)
                            .icon_size(IconSize::XSmall)
                            .tooltip(Tooltip::text("Delete thread"))
                            .on_click(cx.listener(move |this, _, _, cx| {
                                if let Some(session) = this.session.clone() {
                                    if let Err(e) = session.delete_thread(&delete_id) {
                                        this.show_notice(ferrum_ui::Notice::error(e), cx);
                                    }
                                    this.threads = session.threads();
                                }
                                cx.notify();
                            })),
                    )
                    .on_click(cx.listener(move |this, _, window, cx| {
                        if let Some(session) = this.session.clone()
                            && let Err(e) = session.open_thread(&id)
                        {
                            this.show_notice(ferrum_ui::Notice::error(e), cx);
                        }
                        this.show_threads = false;
                        this.refocus(window, cx);
                        cx.notify();
                    }))
            }))
    }
}

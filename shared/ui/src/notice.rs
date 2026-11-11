use std::time::Duration;

use gpui::{AnyElement, App, Context, SharedString, Task, div, prelude::*, px};
use theme::ActiveTheme;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NoticeTone {
    Info,
    Error,
}

/// A transient message shown in the bottom-right corner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Notice {
    pub message: SharedString,
    pub tone: NoticeTone,
}

impl Notice {
    pub const DISMISS_AFTER: Duration = Duration::from_secs(5);

    pub fn info(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
            tone: NoticeTone::Info,
        }
    }

    pub fn error(message: impl Into<SharedString>) -> Self {
        Self {
            message: message.into(),
            tone: NoticeTone::Error,
        }
    }

    /// Schedules `clear` on the view after [`Notice::DISMISS_AFTER`]. Store the
    /// returned task (replacing the previous one) so a newer notice resets the timer.
    pub fn dismiss_later<V: 'static>(
        cx: &mut Context<V>,
        clear: impl FnOnce(&mut V, &mut Context<V>) + 'static,
    ) -> Task<()> {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Self::DISMISS_AFTER).await;
            if let Err(error) = this.update(cx, |view, cx| {
                clear(view, cx);
                cx.notify();
            }) {
                eprintln!("notice: view dropped before dismiss: {error:#}");
            }
        })
    }
}

/// Renders the notice as an absolutely positioned toast; wrap in the root
/// view's `.children(...)`. Clicking calls `on_dismiss`.
pub fn render_notice(
    notice: &Notice,
    on_dismiss: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut App) + 'static,
    cx: &App,
) -> AnyElement {
    let colors = cx.theme().colors();
    let status = cx.theme().status();
    let (background, border, text) = match notice.tone {
        NoticeTone::Info => (
            colors.elevated_surface_background,
            colors.border,
            colors.text,
        ),
        NoticeTone::Error => (status.error_background, status.error_border, colors.text),
    };
    div()
        .id("ferrum-notice")
        .absolute()
        .bottom(px(40.))
        .right(px(16.))
        .max_w(px(448.))
        .px_4()
        .py_3()
        .rounded_md()
        .border_1()
        .shadow_lg()
        .bg(background)
        .border_color(border)
        .text_color(text)
        .text_sm()
        .cursor_pointer()
        .child(notice.message.clone())
        .on_click(on_dismiss)
        .into_any_element()
}

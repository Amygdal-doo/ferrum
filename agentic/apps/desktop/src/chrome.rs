//! The window: project bar, top bar with the column toggles, columns and
//! notices.

use ferrum_ui::components::IconSize;
use ferrum_ui::prelude::*;

use crate::app::{FerrumAgentic, KEY_CONTEXT, ToggleChat, ToggleGit, ToggleTerm};
use crate::columns::id;
use crate::layout::Column;

impl FerrumAgentic {
    pub(crate) fn top_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let toggles = Column::ALL.into_iter().enumerate().map(|(i, column)| {
            let visible = self.layout.is_visible(column);
            let verb = if visible { "Hide" } else { "Show" };
            Button::new(id("toggle", column), column.label())
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .toggle_state(visible)
                .tooltip(Tooltip::text(format!(
                    "{verb} {} (⌥⌘{})",
                    column.title(),
                    i + 1
                )))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.on_toggle(column, window, cx);
                }))
        });
        ferrum_ui::toolbar(cx)
            .child(
                Icon::new(IconName::Sparkle)
                    .size(IconSize::Small)
                    .color(Color::Accent),
            )
            .child(Label::new("FerrumAgentic").weight(gpui::FontWeight::SEMIBOLD))
            .child(div().flex_1())
            .children(toggles)
    }
}

impl Render for FerrumAgentic {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
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
            .bg(colors.background)
            .text_color(colors.text)
            .child(self.project_bar.clone())
            .child(self.top_bar(cx))
            .child(self.columns(cx))
            .children(notice)
    }
}

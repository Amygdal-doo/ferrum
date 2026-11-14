//! History grouped by day; click to reopen in a new tab.

use super::section_header;
use crate::app::FerrumApi;
use crate::components::{method_label, status_color};
use crate::state::history::{group_by_local_day, time_of_day};
use crate::state::{Dialog, tabs};
use ferrum_ui::components::{
    ActiveTheme as _, Button, ButtonCommon as _, ButtonSize, ButtonStyle, Clickable as _, Color,
    IconButton, IconName, IconSize, Label, LabelCommon as _, LabelSize, StyledTypography as _,
    Tooltip, VisibleOnHover as _, h_flex, v_flex,
};
use ferrumapi_domain::HistoryEntry;
use ferrumapi_domain::text::human_duration_ms;
use gpui::{AnyElement, Context, SharedString, div, prelude::*, px};

impl FerrumApi {
    pub(super) fn render_history_view(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let empty = self.state.history.is_empty();
        let groups = group_by_local_day(&self.state.history);
        let clear = (!empty).then(|| {
            Button::new("clear-history", "Clear")
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .tooltip(Tooltip::text("Clear all history"))
                .on_click(cx.listener(|this, _, _, cx| {
                    this.state.dialog = Some(Dialog::ClearHistory);
                    cx.notify();
                }))
                .into_any_element()
        });

        v_flex()
            .size_full()
            .child(section_header("History", clear.into_iter().collect()))
            .child(
                v_flex()
                    .id("history-list")
                    .flex_1()
                    .min_h_0()
                    .overflow_y_scroll()
                    .px_2()
                    .pb_2()
                    .when(empty, |this| {
                        this.child(
                            div().px_2().py_6().flex().justify_center().child(
                                Label::new("Requests you send appear here.")
                                    .size(LabelSize::XSmall)
                                    .color(Color::Muted),
                            ),
                        )
                    })
                    .children(groups.into_iter().map(|group| {
                        v_flex()
                            .mb_2()
                            .child(
                                div()
                                    .px_2()
                                    .py_1()
                                    .text_xs()
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_color(colors.text_muted)
                                    .child(group.label),
                            )
                            .children(group.entries.iter().map(|e| self.render_history_row(e, cx)))
                    })),
            )
            .into_any_element()
    }

    fn render_history_row(&self, entry: &HistoryEntry, cx: &mut Context<Self>) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let id = entry.id;
        let label = entry.request.label();
        let (status, status_hsla) = match (entry.status, &entry.error) {
            (Some(s), _) => (s.to_string(), status_color(s, cx)),
            (None, Some(_)) => ("ERR".to_owned(), cx.theme().status().error),
            (None, None) => (String::new(), colors.text_muted),
        };
        let meta = format!(
            "{}{}",
            time_of_day(entry.sent_at),
            entry
                .time_ms
                .map(|t| format!(" · {}", human_duration_ms(t)))
                .unwrap_or_default()
        );
        let tooltip = entry.error.clone().unwrap_or_else(|| label.clone());
        let reopen = entry.clone();
        let group: SharedString = format!("history-{id}").into();

        h_flex()
            .id(SharedString::from(format!("history-row-{id}")))
            .group(group.clone())
            .gap_2()
            .px_2()
            .py_1()
            .rounded_md()
            .hover(|s| s.bg(colors.element_hover))
            .tooltip(Tooltip::text(tooltip))
            .on_click(cx.listener(move |this, _, _, cx| {
                // Always a new tab so the history snapshot never overwrites an open one.
                let mut tab = tabs::blank_tab();
                tab.request = reopen.request.clone();
                tab.origin = reopen
                    .origin
                    .filter(|o| tabs::find_by_origin(&this.state.tabs, *o).is_none());
                tab.dirty = tab.origin.is_some();
                this.push_tab(tab);
                cx.notify();
            }))
            .child(
                div()
                    .w(px(44.))
                    .child(method_label(entry.request.method, cx)),
            )
            .child(
                v_flex()
                    .flex_1()
                    .min_w_0()
                    .child(div().truncate().text_xs().font_buffer(cx).child(label))
                    .child(Label::new(meta).size(LabelSize::XSmall).color(Color::Muted)),
            )
            .child(
                div()
                    .text_xs()
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .text_color(status_hsla)
                    .child(status),
            )
            .child(
                div().visible_on_hover(group).child(
                    IconButton::new(
                        SharedString::from(format!("history-delete-{id}")),
                        IconName::Close,
                    )
                    .icon_size(IconSize::XSmall)
                    .tooltip(Tooltip::text("Delete history entry"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.delete_history(id, cx);
                        cx.notify();
                    })),
                ),
            )
            .into_any_element()
    }
}

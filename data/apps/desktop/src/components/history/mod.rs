//! History panel: searchable list of executed queries for the active tab's
//! connection (or every connection), with Open and Run per entry.

use crate::app::FerrumData;
use crate::commands::history::HISTORY_PAGE;
use crate::state::Dialog;
use ferrum_ui::prelude::*;
use ferrumdata_domain::{HistoryEntry, HistoryStatus};
use std::time::Duration;

/// Characters of query text shown inline and in the tooltip.
const PREVIEW_CHARS: usize = 240;
const TOOLTIP_CHARS: usize = 2_000;

impl FerrumData {
    pub fn render_history(&mut self, cx: &mut Context<Self>) -> AnyElement {
        self.refresh_history_if_stale(cx);
        let colors = cx.theme().colors().clone();
        let all = self.state.history.all_connections;
        let active_profile = self.state.active_profile().cloned();
        let scoped = all || active_profile.is_some();
        let history = &self.state.history;
        let count = history.entries.len();

        let scope_button = |id: &'static str, label: &'static str, value: bool| {
            Button::new(id, label)
                .label_size(LabelSize::Small)
                .toggle_state(all == value)
                .selected_style(ButtonStyle::Filled)
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.state.history.all_connections = value;
                    cx.notify();
                }))
        };
        let clear_target = if all { None } else { active_profile.clone() };
        let toolbar = h_flex()
            .flex_none()
            .gap_2()
            .px_3()
            .py_2()
            .border_b_1()
            .border_color(colors.border)
            .child(div().w(px(280.)).child(self.history_filter.clone()))
            .child(scope_button("history-this", "This connection", false))
            .child(scope_button("history-all", "All connections", true))
            .child(div().flex_1())
            .when(count > 0, |this| {
                let more = if count >= HISTORY_PAGE { "+" } else { "" };
                this.child(
                    Label::new(format!("{count}{more} queries"))
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
            })
            .child(
                Button::new("history-clear", "Clear…")
                    .label_size(LabelSize::Small)
                    .disabled(!scoped || count == 0)
                    .tooltip(Tooltip::text(if all {
                        "Delete the history of every connection"
                    } else {
                        "Delete this connection's history"
                    }))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.state.dialog = Some(Dialog::ClearHistory(clear_target.clone()));
                        cx.notify();
                    })),
            );

        let message = |text: &str| {
            div()
                .p_4()
                .child(
                    Label::new(text.to_owned())
                        .size(LabelSize::Small)
                        .color(Color::Muted),
                )
                .into_any_element()
        };
        let body = if !scoped {
            message("Choose a connection for this tab, or show all connections.")
        } else if let Some(error) = &history.error {
            div()
                .p_4()
                .text_sm()
                .text_color(cx.theme().status().error)
                .child(error.clone())
                .into_any_element()
        } else if history.entries.is_empty() && history.loading {
            message("Loading history…")
        } else if history.entries.is_empty() {
            let searching = !self.history_filter.read(cx).text().trim().is_empty();
            message(if searching {
                "No queries match the search."
            } else {
                "No queries run yet. Every statement you run is recorded here."
            })
        } else {
            let entries = history.entries.clone();
            v_flex()
                .id("history-list")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .children(
                    entries
                        .iter()
                        .map(|entry| self.render_history_entry(entry, all, cx)),
                )
                .into_any_element()
        };

        v_flex()
            .absolute()
            .inset_0()
            .child(toolbar)
            .child(body)
            .into_any_element()
    }

    fn render_history_entry(
        &self,
        entry: &HistoryEntry,
        show_connection: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let colors = cx.theme().colors().clone();
        let status = cx.theme().status().clone();
        let id = entry.id;
        let when = entry
            .executed_at
            .with_timezone(&chrono::Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string();
        let connection = self.state.profile(entry.profile_id).map_or_else(
            || "deleted connection".to_owned(),
            |p| p.display_name.clone(),
        );
        let summary_color = match entry.status {
            HistoryStatus::Completed => colors.text_muted,
            HistoryStatus::Failed => status.error,
            HistoryStatus::Cancelled | HistoryStatus::TimedOut => status.warning,
        };
        let preview: String = entry
            .sql
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" ")
            .chars()
            .take(PREVIEW_CHARS)
            .collect();
        let mut tooltip: String = entry.sql.chars().take(TOOLTIP_CHARS).collect();
        if let Some(error) = &entry.error {
            tooltip.push_str("\n\n");
            tooltip.push_str(error);
        }
        let open = entry.clone();
        let rerun = entry.clone();
        let double_click = entry.clone();

        h_flex()
            .id(SharedString::from(format!("history-{id}")))
            .flex_none()
            .gap_3()
            .px_3()
            .py_1p5()
            .border_b_1()
            .border_color(colors.border_variant)
            .hover(|s| s.bg(colors.element_hover))
            .on_click(cx.listener(move |this, e: &ClickEvent, _, cx| {
                if e.click_count() >= 2 {
                    this.open_history_entry(&double_click, false, cx);
                }
            }))
            .child(
                v_flex()
                    .id(SharedString::from(format!("history-text-{id}")))
                    .flex_1()
                    .min_w_0()
                    .gap_0p5()
                    .tooltip(Tooltip::text(tooltip))
                    .child(
                        h_flex()
                            .gap_3()
                            .text_xs()
                            .text_color(colors.text_muted)
                            .child(when)
                            .when(show_connection, |this| this.child(connection))
                            .child(div().text_color(summary_color).child(entry.summary()))
                            .child(format_duration(entry.duration)),
                    )
                    .child(div().truncate().font_buffer(cx).text_xs().child(preview)),
            )
            .child(
                Button::new(SharedString::from(format!("history-open-{id}")), "Open")
                    .label_size(LabelSize::Small)
                    .tooltip(Tooltip::text("Open in a new editor tab (or double-click)"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_history_entry(&open, false, cx);
                    })),
            )
            .child(
                Button::new(SharedString::from(format!("history-run-{id}")), "Run")
                    .label_size(LabelSize::Small)
                    .style(ButtonStyle::Outlined)
                    .tooltip(Tooltip::text("Open in a new tab and run it again"))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.open_history_entry(&rerun, true, cx);
                    })),
            )
            .into_any_element()
    }
}

/// `850 ms`, `12.3 s`, `4 min 05 s`.
fn format_duration(d: Duration) -> String {
    let ms = d.as_millis();
    if ms < 10_000 {
        format!("{ms} ms")
    } else if ms < 60_000 {
        format!("{:.1} s", d.as_secs_f64())
    } else {
        let s = d.as_secs();
        format!("{} min {:02} s", s / 60, s % 60)
    }
}

#[cfg(test)]
mod tests {
    use super::format_duration;
    use std::time::Duration;

    #[test]
    fn durations_are_compact() {
        assert_eq!(format_duration(Duration::from_millis(850)), "850 ms");
        assert_eq!(format_duration(Duration::from_millis(12_345)), "12.3 s");
        assert_eq!(format_duration(Duration::from_secs(245)), "4 min 05 s");
    }
}

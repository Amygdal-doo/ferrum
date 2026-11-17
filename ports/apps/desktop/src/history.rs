//! The History view (header toggle): which process held which port and
//! when on this Mac, newest first, filtered by the search field.

use std::time::{Duration, Instant};

use chrono::{Local, Utc};
use ferrum_ui::prelude::*;
use ferrumports_core::history::{HistoryEntry, MAX_AGE_DAYS, MAX_ENTRIES};

use crate::app::FerrumPorts;

/// The file is rewritten when an entry opens or closes, else at most this
/// often (for `last_seen`).
const SAVE_EVERY: Duration = Duration::from_secs(60);

/// (title, width); `None` is the flexible command column.
const COLUMNS: [(&str, Option<f32>); 7] = [
    ("Port", Some(78.)),
    ("Proto", Some(52.)),
    ("Process", Some(130.)),
    ("PID", Some(66.)),
    ("Project", Some(120.)),
    ("When", Some(230.)),
    ("Command", None),
];

impl FerrumPorts {
    /// Records the listeners of a scan of this Mac in the history.
    pub fn record_history(&mut self) {
        let changed = self.history.observe(&self.listeners, Utc::now());
        let due = self
            .history_saved
            .is_none_or(|at| at.elapsed() >= SAVE_EVERY);
        if changed || due {
            self.save_history();
        }
    }

    pub fn save_history(&mut self) {
        let Some(path) = self.history_path.as_deref() else {
            return;
        };
        self.history_saved = Some(Instant::now());
        if let Err(e) = self.history.save(path) {
            // Reported once in the footer; recording goes on in memory.
            self.warnings.push(e);
        }
    }

    pub fn clear_history(&mut self, cx: &mut Context<Self>) {
        self.history.clear();
        self.save_history();
        cx.notify();
    }
}

fn cell(width: Option<f32>, child: impl IntoElement) -> Div {
    let base = match width {
        Some(w) => div().flex_none().w(px(w)),
        None => div().flex_1().min_w_0(),
    };
    base.overflow_hidden().pr_2().child(child)
}

fn text(text: impl Into<SharedString>, color: Color) -> Label {
    Label::new(text.into())
        .size(LabelSize::Small)
        .color(color)
        .truncate()
}

/// `Today 09:12 – 09:40 · 28 min`, `Nov 16 22:01 – now`.
fn when(entry: &HistoryEntry) -> String {
    let today = Local::now().date_naive();
    let first = entry.first_seen.with_timezone(&Local);
    let last = entry.last_seen.with_timezone(&Local);
    let day = |t: chrono::DateTime<Local>| {
        if t.date_naive() == today {
            t.format("%H:%M").to_string()
        } else {
            t.format("%b %-d %H:%M").to_string()
        }
    };
    let end = if entry.open {
        "now".to_string()
    } else if last.date_naive() == first.date_naive() {
        last.format("%H:%M").to_string()
    } else {
        day(last)
    };
    format!("{} – {end} · {}", day(first), entry.duration())
}

fn row(entry: &HistoryEntry, index: usize, cx: &App) -> AnyElement {
    let colors = cx.theme().colors();
    h_flex()
        .id(("history", index))
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(colors.border_variant)
        .child(cell(
            COLUMNS[0].1,
            Label::new(entry.port.to_string())
                .size(LabelSize::Small)
                .weight(gpui::FontWeight::SEMIBOLD),
        ))
        .child(cell(
            COLUMNS[1].1,
            text(entry.protocol().label(), Color::Muted),
        ))
        .child(cell(
            COLUMNS[2].1,
            text(entry.process.clone(), Color::Default),
        ))
        .child(cell(
            COLUMNS[3].1,
            text(
                entry.pid.map_or("—".to_string(), |p| p.to_string()),
                Color::Muted,
            ),
        ))
        .child(cell(
            COLUMNS[4].1,
            text(entry.project_name().unwrap_or_default(), Color::Accent),
        ))
        .child(cell(
            COLUMNS[5].1,
            text(
                when(entry),
                if entry.open {
                    Color::Success
                } else {
                    Color::Muted
                },
            ),
        ))
        .child(cell(
            COLUMNS[6].1,
            text(entry.command.clone().unwrap_or_default(), Color::Muted),
        ))
        .into_any_element()
}

pub fn render(app: &FerrumPorts, cx: &mut Context<FerrumPorts>) -> AnyElement {
    let colors = cx.theme().colors().clone();
    let query = app.search.read(cx).text().to_string();
    let rows: Vec<AnyElement> = app
        .history
        .entries()
        .filter(|e| e.matches(&query))
        .enumerate()
        .map(|(i, e)| row(e, i, cx))
        .collect();
    let header = h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .border_b_1()
        .border_color(colors.border_variant)
        .children(
            COLUMNS
                .iter()
                .map(|(title, width)| cell(*width, text(*title, Color::Muted))),
        );
    let body = if app.history.is_empty() {
        empty_state(
            "No history yet",
            "Ports of this Mac are recorded here while FerrumPorts runs",
        )
        .into_any_element()
    } else if rows.is_empty() {
        empty_state("Nothing matches", "Clear the search").into_any_element()
    } else {
        div()
            .id("history-rows")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .children(rows)
            .into_any_element()
    };
    let note = h_flex()
        .flex_none()
        .px_3()
        .py_1()
        .gap_2()
        .border_b_1()
        .border_color(colors.border_variant)
        .child(
            div().flex_1().min_w_0().child(
                Label::new(format!(
                    "This Mac, recorded while FerrumPorts runs · last {MAX_AGE_DAYS} days, \
                     up to {MAX_ENTRIES} entries · ~/.ferrum/ports/history.json"
                ))
                .size(LabelSize::XSmall)
                .color(Color::Muted)
                .truncate(),
            ),
        )
        .child(
            Button::new("clear-history", "Clear")
                .style(ButtonStyle::Subtle)
                .size(ButtonSize::Compact)
                .label_size(LabelSize::Small)
                .disabled(app.history.is_empty())
                .on_click(cx.listener(|app, _, _, cx| app.clear_history(cx))),
        );
    v_flex()
        .flex_1()
        .min_w_0()
        .h_full()
        .child(note)
        .child(header)
        .child(v_flex().flex_1().min_h_0().child(body))
        .into_any_element()
}

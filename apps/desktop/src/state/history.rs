//! Grouping history entries by local calendar day.

use chrono::{DateTime, Local, NaiveDate, Utc};
use ferrumapi_domain::HistoryEntry;

#[derive(Debug, Clone, PartialEq)]
pub struct DayGroup {
    pub label: String,
    pub entries: Vec<HistoryEntry>,
}

/// "Today", "Yesterday" or e.g. "Mon, Oct 20 2026".
#[must_use]
pub fn day_label(day: NaiveDate, today: NaiveDate) -> String {
    if day == today {
        "Today".to_owned()
    } else if today.pred_opt() == Some(day) {
        "Yesterday".to_owned()
    } else {
        day.format("%a, %b %-d %Y").to_string()
    }
}

/// Groups entries (newest first in, newest first out) by the day returned
/// by `day_of`. Pure so it can be tested without a time zone.
#[must_use]
pub fn group_by<F>(entries: &[HistoryEntry], today: NaiveDate, day_of: F) -> Vec<DayGroup>
where
    F: Fn(DateTime<Utc>) -> NaiveDate,
{
    let mut groups: Vec<(NaiveDate, Vec<HistoryEntry>)> = Vec::new();
    for e in entries {
        let day = day_of(e.sent_at);
        match groups.iter_mut().find(|(d, _)| *d == day) {
            Some((_, list)) => list.push(e.clone()),
            None => groups.push((day, vec![e.clone()])),
        }
    }
    groups.sort_by_key(|g| std::cmp::Reverse(g.0));
    groups
        .into_iter()
        .map(|(day, entries)| DayGroup {
            label: day_label(day, today),
            entries,
        })
        .collect()
}

/// Groups by the local calendar day.
#[must_use]
pub fn group_by_local_day(entries: &[HistoryEntry]) -> Vec<DayGroup> {
    let today = Local::now().date_naive();
    group_by(entries, today, |t| t.with_timezone(&Local).date_naive())
}

/// Local wall-clock time, e.g. `14:05`.
#[must_use]
pub fn time_of_day(t: DateTime<Utc>) -> String {
    t.with_timezone(&Local).format("%H:%M").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use ferrumapi_domain::HttpRequest;

    fn entry_at(y: i32, m: u32, d: u32, h: u32) -> HistoryEntry {
        let mut e = HistoryEntry::new(HttpRequest::default(), None);
        e.sent_at = Utc
            .with_ymd_and_hms(y, m, d, h, 0, 0)
            .single()
            .unwrap_or_default();
        e
    }

    #[test]
    fn groups_newest_day_first_with_labels() {
        let entries = vec![
            entry_at(2026, 10, 22, 9),
            entry_at(2026, 10, 22, 8),
            entry_at(2026, 10, 21, 23),
            entry_at(2026, 10, 18, 12),
        ];
        let today = NaiveDate::from_ymd_opt(2026, 10, 22).unwrap_or_default();
        let groups = group_by(&entries, today, |t| t.date_naive());
        let labels: Vec<_> = groups.iter().map(|g| g.label.as_str()).collect();
        assert_eq!(labels, ["Today", "Yesterday", "Sun, Oct 18 2026"]);
        assert_eq!(groups[0].entries.len(), 2);
    }

    #[test]
    fn empty_history_has_no_groups() {
        let today = NaiveDate::from_ymd_opt(2026, 1, 1).unwrap_or_default();
        assert!(group_by(&[], today, |t| t.date_naive()).is_empty());
    }
}

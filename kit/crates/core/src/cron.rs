//! Cron expressions: parse the classic five fields (minute hour
//! day-of-month month day-of-week), explain them in English and list the
//! next run times.
//!
//! Supported: `*`, values, ranges `a-b`, steps `*/n` `a-b/n` `a/n`, lists,
//! month and weekday names (`JAN`, `MON`), `7` as Sunday, `?` as `*`, and
//! the macros `@yearly` `@annually` `@monthly` `@weekly` `@daily`
//! `@midnight` `@hourly`. Like Vixie cron, when both day-of-month and
//! day-of-week are restricted, a day matching either one runs.

use chrono::{Datelike, Duration, NaiveDate, NaiveDateTime, NaiveTime};

const MONTHS: [&str; 12] = [
    "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
];
const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
const MONTH_NAMES: [&str; 12] = [
    "January",
    "February",
    "March",
    "April",
    "May",
    "June",
    "July",
    "August",
    "September",
    "October",
    "November",
    "December",
];
const DAY_NAMES: [&str; 7] = [
    "Sunday",
    "Monday",
    "Tuesday",
    "Wednesday",
    "Thursday",
    "Friday",
    "Saturday",
];
/// How far ahead [`next_runs`] looks (covers 29 February).
const SEARCH_DAYS: i64 = 366 * 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Kind {
    Minute,
    Hour,
    DayOfMonth,
    Month,
    DayOfWeek,
}

impl Kind {
    fn range(self) -> (u32, u32) {
        match self {
            Kind::Minute => (0, 59),
            Kind::Hour => (0, 23),
            Kind::DayOfMonth => (1, 31),
            Kind::Month => (1, 12),
            // 7 is accepted as Sunday and folded to 0.
            Kind::DayOfWeek => (0, 7),
        }
    }

    fn label(self) -> &'static str {
        match self {
            Kind::Minute => "minute",
            Kind::Hour => "hour",
            Kind::DayOfMonth => "day of month",
            Kind::Month => "month",
            Kind::DayOfWeek => "day of week",
        }
    }
}

/// The values one field allows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Field {
    bits: u64,
    /// Written as `*` (or `?`): matters for the day-of-month / day-of-week rule.
    pub any: bool,
}

impl Field {
    #[must_use]
    pub fn contains(&self, value: u32) -> bool {
        value < 64 && self.bits & (1 << value) != 0
    }

    #[must_use]
    pub fn values(&self) -> Vec<u32> {
        (0..64).filter(|v| self.contains(*v)).collect()
    }
}

/// A parsed expression.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cron {
    pub minute: Field,
    pub hour: Field,
    pub day_of_month: Field,
    pub month: Field,
    pub day_of_week: Field,
}

fn parse_value(text: &str, kind: Kind) -> Result<u32, String> {
    let upper = text.to_ascii_uppercase();
    let named = match kind {
        Kind::Month => MONTHS
            .iter()
            .position(|m| *m == upper)
            .map(|i| i as u32 + 1),
        Kind::DayOfWeek => DAYS.iter().position(|d| *d == upper).map(|i| i as u32),
        _ => None,
    };
    let value = match named {
        Some(value) => value,
        None => text
            .parse::<u32>()
            .map_err(|_| format!("“{text}” is not a valid {}", kind.label()))?,
    };
    let (min, max) = kind.range();
    if value < min || value > max {
        return Err(format!(
            "{} {value} is out of range ({min}–{max})",
            kind.label()
        ));
    }
    Ok(value)
}

fn parse_field(text: &str, kind: Kind) -> Result<Field, String> {
    let (min, max) = kind.range();
    let mut bits = 0u64;
    let any = text == "*" || text == "?";
    for part in text.split(',') {
        if part.is_empty() {
            return Err(format!("empty list item in the {} field", kind.label()));
        }
        let (range, step) = match part.split_once('/') {
            Some((range, step)) => {
                let step = step
                    .parse::<u32>()
                    .ok()
                    .filter(|s| *s > 0)
                    .ok_or_else(|| format!("“{step}” is not a valid step"))?;
                (range, step)
            }
            None => (part, 1),
        };
        let (start, end) = if range == "*" || range == "?" {
            (min, max)
        } else if let Some((a, b)) = range.split_once('-') {
            let (a, b) = (parse_value(a, kind)?, parse_value(b, kind)?);
            if a > b {
                return Err(format!("range {a}-{b} runs backwards"));
            }
            (a, b)
        } else {
            let start = parse_value(range, kind)?;
            // `5/15` means from 5 to the end, every 15.
            (start, if step > 1 { max } else { start })
        };
        let mut value = start;
        while value <= end {
            bits |= 1 << value;
            value += step;
        }
    }
    if kind == Kind::DayOfWeek && bits & (1 << 7) != 0 {
        bits = (bits & !(1 << 7)) | 1;
    }
    Ok(Field { bits, any })
}

/// Parses a five-field expression or a macro.
pub fn parse(expression: &str) -> Result<Cron, String> {
    let expression = expression.trim();
    let expanded = match expression.to_ascii_lowercase().as_str() {
        "@yearly" | "@annually" => "0 0 1 1 *",
        "@monthly" => "0 0 1 * *",
        "@weekly" => "0 0 * * 0",
        "@daily" | "@midnight" => "0 0 * * *",
        "@hourly" => "0 * * * *",
        "@reboot" => return Err("@reboot runs once at startup; it has no schedule".into()),
        other if other.starts_with('@') => return Err(format!("unknown macro {expression}")),
        _ => expression,
    };
    let fields: Vec<&str> = expanded.split_whitespace().collect();
    if fields.len() != 5 {
        return Err(format!(
            "expected 5 fields (minute hour day-of-month month day-of-week), found {}",
            fields.len()
        ));
    }
    Ok(Cron {
        minute: parse_field(fields[0], Kind::Minute)?,
        hour: parse_field(fields[1], Kind::Hour)?,
        day_of_month: parse_field(fields[2], Kind::DayOfMonth)?,
        month: parse_field(fields[3], Kind::Month)?,
        day_of_week: parse_field(fields[4], Kind::DayOfWeek)?,
    })
}

impl Cron {
    /// Whether the job runs on `date` (time of day aside).
    #[must_use]
    pub fn runs_on(&self, date: NaiveDate) -> bool {
        if !self.month.contains(date.month()) {
            return false;
        }
        let dom = self.day_of_month.contains(date.day());
        let dow = self
            .day_of_week
            .contains(date.weekday().num_days_from_sunday());
        match (self.day_of_month.any, self.day_of_week.any) {
            (true, true) => true,
            (false, true) => dom,
            (true, false) => dow,
            (false, false) => dom || dow,
        }
    }
}

/// The next `count` run times strictly after `after` (local wall-clock
/// time, minute precision). Fewer when none come within eight years.
#[must_use]
pub fn next_runs(cron: &Cron, after: NaiveDateTime, count: usize) -> Vec<NaiveDateTime> {
    let mut runs = Vec::new();
    let hours = cron.hour.values();
    let minutes = cron.minute.values();
    let mut date = after.date();
    for _ in 0..SEARCH_DAYS {
        if cron.runs_on(date) {
            for hour in &hours {
                for minute in &minutes {
                    let Some(time) = NaiveTime::from_hms_opt(*hour, *minute, 0) else {
                        continue;
                    };
                    let at = date.and_time(time);
                    if at > after {
                        runs.push(at);
                        if runs.len() == count {
                            return runs;
                        }
                    }
                }
            }
        }
        date += Duration::days(1);
    }
    runs
}

/// `a, b and c`.
fn join(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

/// Values as runs: `1 through 5`, single values listed.
fn ranges(values: &[u32], name: &dyn Fn(u32) -> String) -> String {
    let mut parts = Vec::new();
    let mut i = 0;
    while i < values.len() {
        let mut j = i;
        while j + 1 < values.len() && values[j + 1] == values[j] + 1 {
            j += 1;
        }
        if j - i >= 2 {
            parts.push(format!("{} through {}", name(values[i]), name(values[j])));
        } else {
            parts.extend(values[i..=j].iter().map(|v| name(*v)));
        }
        i = j + 1;
    }
    join(&parts)
}

/// `Some(step)` when `values` are every `step` from the field's minimum.
fn regular_step(values: &[u32], min: u32, max: u32) -> Option<u32> {
    let step = values.get(1)?.checked_sub(values[0])?;
    let regular = step > 1
        && values[0] == min
        && values.windows(2).all(|w| w[1] - w[0] == step)
        && values.last().is_some_and(|last| last + step > max);
    regular.then_some(step)
}

fn time_part(cron: &Cron) -> String {
    let minutes = cron.minute.values();
    let hours = cron.hour.values();
    let all_hours = hours.len() == 24;
    if minutes.len() == 60 && all_hours {
        return "Every minute".into();
    }
    if minutes.len() == 60 {
        return format!(
            "Every minute during hour {}",
            ranges(&hours, &|h| format!("{h:02}"))
        );
    }
    let minute_text = match regular_step(&minutes, 0, 59) {
        Some(step) => format!("Every {step} minutes"),
        None => String::new(),
    };
    if all_hours {
        return match (minute_text.is_empty(), minutes.as_slice()) {
            (false, _) => minute_text,
            (true, [0]) => "Every hour, on the hour".into(),
            (true, [m]) => format!("At minute {m} of every hour"),
            (true, _) => format!(
                "At minutes {} of every hour",
                ranges(&minutes, &|m| m.to_string())
            ),
        };
    }
    if !minute_text.is_empty() {
        return format!(
            "{minute_text} during hour {}",
            ranges(&hours, &|h| format!("{h:02}"))
        );
    }
    if minutes.len() * hours.len() <= 6 {
        let times: Vec<String> = hours
            .iter()
            .flat_map(|h| minutes.iter().map(move |m| format!("{h:02}:{m:02}")))
            .collect();
        return format!("At {}", join(&times));
    }
    if let Some(step) = regular_step(&hours, 0, 23) {
        return format!(
            "At minute {} of every {step} hours",
            ranges(&minutes, &|m| m.to_string())
        );
    }
    format!(
        "At minute {} past hour {}",
        ranges(&minutes, &|m| m.to_string()),
        ranges(&hours, &|h| format!("{h:02}"))
    )
}

fn ordinal(n: u32) -> String {
    let suffix = match (n % 10, n % 100) {
        (1, 11) | (2, 12) | (3, 13) => "th",
        (1, _) => "st",
        (2, _) => "nd",
        (3, _) => "rd",
        _ => "th",
    };
    format!("{n}{suffix}")
}

/// An English description, e.g. `At 09:30, Monday through Friday`.
#[must_use]
pub fn explain(cron: &Cron) -> String {
    let mut text = time_part(cron);
    let dom = (!cron.day_of_month.any).then(|| {
        let days = cron.day_of_month.values();
        match regular_step(&days, 1, 31) {
            Some(step) => format!("every {step} days of the month"),
            None => format!("on the {} of the month", ranges(&days, &|d| ordinal(d))),
        }
    });
    let dow = (!cron.day_of_week.any).then(|| {
        let days = cron.day_of_week.values();
        if days.len() == 7 {
            "every day of the week".to_string()
        } else {
            format!(
                "on {}",
                ranges(&days, &|d| DAY_NAMES[d as usize].to_string())
            )
        }
    });
    match (dom, dow) {
        (Some(dom), Some(dow)) => text.push_str(&format!(", {dom} or {dow}")),
        (Some(day), None) | (None, Some(day)) => text.push_str(&format!(", {day}")),
        (None, None) => {}
    }
    let months = cron.month.values();
    if months.len() < 12 {
        text.push_str(&format!(
            ", in {}",
            ranges(&months, &|m| MONTH_NAMES[m as usize - 1].to_string())
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(text: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M").unwrap()
    }

    fn explained(expression: &str) -> String {
        explain(&parse(expression).unwrap())
    }

    #[test]
    fn explains_common_expressions() {
        assert_eq!(explained("* * * * *"), "Every minute");
        assert_eq!(explained("*/15 * * * *"), "Every 15 minutes");
        assert_eq!(explained("@hourly"), "Every hour, on the hour");
        assert_eq!(explained("5 * * * *"), "At minute 5 of every hour");
        assert_eq!(
            explained("30 9 * * 1-5"),
            "At 09:30, on Monday through Friday"
        );
        assert_eq!(explained("0 9,18 * * *"), "At 09:00 and 18:00");
        assert_eq!(
            explained("0 0 1 JAN,JUL *"),
            "At 00:00, on the 1st of the month, in January and July"
        );
        assert_eq!(
            explained("0 12 13 * FRI"),
            "At 12:00, on the 13th of the month or on Friday"
        );
        assert_eq!(explained("0 */6 * * *"), "At 00:00, 06:00, 12:00 and 18:00");
        assert_eq!(explained("0 */2 * * *"), "At minute 0 of every 2 hours");
        assert_eq!(explained("@weekly"), "At 00:00, on Sunday");
        assert_eq!(explained("0 0 * * 7"), "At 00:00, on Sunday");
    }

    #[test]
    fn rejects_bad_expressions() {
        assert!(parse("* * * *").unwrap_err().contains("5 fields"));
        assert!(parse("60 * * * *").unwrap_err().contains("out of range"));
        assert!(parse("5-1 * * * *").unwrap_err().contains("backwards"));
        assert!(parse("*/0 * * * *").is_err());
        assert!(parse("* * * FOO *").is_err());
        assert!(parse("@reboot").is_err());
        assert!(parse("@often").is_err());
    }

    #[test]
    fn lists_next_runs() {
        let cron = parse("30 9 * * 1-5").unwrap();
        // 2026-11-13 is a Friday.
        let runs = next_runs(&cron, at("2026-11-13 10:00"), 3);
        assert_eq!(
            runs,
            [
                at("2026-11-16 09:30"),
                at("2026-11-17 09:30"),
                at("2026-11-18 09:30")
            ]
        );
        let every = next_runs(&parse("*/20 * * * *").unwrap(), at("2026-01-01 00:00"), 2);
        assert_eq!(every, [at("2026-01-01 00:20"), at("2026-01-01 00:40")]);
    }

    #[test]
    fn day_of_month_or_day_of_week() {
        let cron = parse("0 0 13 * 5").unwrap();
        let runs = next_runs(&cron, at("2026-11-01 00:00"), 3);
        // Friday 6th, Friday 13th, Friday 20th.
        assert_eq!(
            runs,
            [
                at("2026-11-06 00:00"),
                at("2026-11-13 00:00"),
                at("2026-11-20 00:00")
            ]
        );
    }

    #[test]
    fn leap_day_and_impossible_dates() {
        let runs = next_runs(&parse("0 0 29 2 *").unwrap(), at("2026-03-01 00:00"), 1);
        assert_eq!(runs, [at("2028-02-29 00:00")]);
        assert!(next_runs(&parse("0 0 31 2 *").unwrap(), at("2026-01-01 00:00"), 1).is_empty());
    }
}

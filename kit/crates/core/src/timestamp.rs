//! Unix timestamp ⇄ date conversion.

use chrono::{DateTime, NaiveDate, NaiveDateTime, TimeZone, Utc};
use std::fmt::Display;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// Values with 12 or more digits are milliseconds, others seconds.
    Auto,
    Seconds,
    Milliseconds,
}

/// Magnitude from which [`Unit::Auto`] reads milliseconds (≈ year 5138 in seconds).
const AUTO_MILLIS_FROM: i64 = 100_000_000_000;

/// Parses an integer Unix timestamp. Underscores, commas and spaces are ignored.
pub fn parse_timestamp(input: &str, unit: Unit) -> Result<(DateTime<Utc>, Unit), String> {
    let cleaned: String = input
        .chars()
        .filter(|c| !matches!(c, '_' | ',' | ' '))
        .collect();
    if cleaned.is_empty() {
        return Err("Enter a timestamp".into());
    }
    let value: i64 = cleaned
        .parse()
        .map_err(|_| format!("'{}' is not a whole number", input.trim()))?;
    let unit = match unit {
        Unit::Auto if value.unsigned_abs() >= AUTO_MILLIS_FROM as u64 => Unit::Milliseconds,
        Unit::Auto => Unit::Seconds,
        other => other,
    };
    let date = match unit {
        Unit::Milliseconds => DateTime::from_timestamp_millis(value),
        _ => DateTime::from_timestamp(value, 0),
    };
    date.map(|d| (d, unit))
        .ok_or_else(|| "Timestamp is out of the supported date range".to_string())
}

/// Parses a date: RFC 3339 / ISO 8601 with offset (`2024-05-01T12:00:00Z`),
/// or `YYYY-MM-DD[ HH:MM[:SS]]` / `YYYY-MM-DDTHH:MM[:SS]` read in `tz`.
pub fn parse_date<Tz: TimeZone>(input: &str, tz: &Tz) -> Result<DateTime<Utc>, String> {
    let input = input.trim();
    if input.is_empty() {
        return Err("Enter a date".into());
    }
    if let Ok(date) = DateTime::parse_from_rfc3339(input) {
        return Ok(date.with_timezone(&Utc));
    }
    if let Ok(date) = DateTime::parse_from_rfc2822(input) {
        return Ok(date.with_timezone(&Utc));
    }
    let naive = [
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%d %H:%M",
        "%Y-%m-%dT%H:%M",
    ]
    .iter()
    .find_map(|format| NaiveDateTime::parse_from_str(input, format).ok())
    .or_else(|| {
        NaiveDate::parse_from_str(input, "%Y-%m-%d")
            .ok()
            .and_then(|d| d.and_hms_opt(0, 0, 0))
    })
    .ok_or_else(|| {
        "Unrecognised date. Use e.g. 2024-05-01 12:30:00 or 2024-05-01T12:30:00Z".to_string()
    })?;
    tz.from_local_datetime(&naive)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
        .ok_or_else(|| "That local time does not exist (daylight-saving gap)".to_string())
}

/// Reads `input` as a Unix timestamp when it is a (signed) whole number,
/// otherwise as a date (see [`parse_date`]). Returns the unit used for
/// timestamps, `None` for dates.
pub fn parse_any<Tz: TimeZone>(
    input: &str,
    unit: Unit,
    tz: &Tz,
) -> Result<(DateTime<Utc>, Option<Unit>), String> {
    let trimmed = input.trim();
    let digits = trimmed.strip_prefix('-').unwrap_or(trimmed);
    let numeric = !digits.is_empty()
        && digits.chars().any(|c| c.is_ascii_digit())
        && digits
            .chars()
            .all(|c| c.is_ascii_digit() || matches!(c, '_' | ','));
    if numeric {
        parse_timestamp(trimmed, unit).map(|(date, unit)| (date, Some(unit)))
    } else {
        parse_date(trimmed, tz).map(|date| (date, None))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TimestampInfo {
    pub seconds: i64,
    pub millis: i64,
    /// `2024-05-01 12:00:00.000 UTC`
    pub utc: String,
    /// The same instant in the given zone, with its offset.
    pub local: String,
    /// RFC 3339 in UTC with milliseconds.
    pub iso: String,
    /// "3 days ago" / "in 2 hours" / "now".
    pub relative: String,
}

pub fn describe<Tz: TimeZone>(date: DateTime<Utc>, tz: &Tz, now: DateTime<Utc>) -> TimestampInfo
where
    Tz::Offset: Display,
{
    TimestampInfo {
        seconds: date.timestamp(),
        millis: date.timestamp_millis(),
        utc: date.format("%Y-%m-%d %H:%M:%S%.3f UTC").to_string(),
        local: date
            .with_timezone(tz)
            .format("%Y-%m-%d %H:%M:%S%.3f %:z (%a)")
            .to_string(),
        iso: date.to_rfc3339_opts(chrono::SecondsFormat::Millis, true),
        relative: relative(date, now),
    }
}

/// Human distance between `date` and `now`, rounded down to the largest unit.
pub fn relative(date: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let delta = date.timestamp() - now.timestamp();
    let magnitude = delta.unsigned_abs();
    if magnitude < 5 {
        return "now".into();
    }
    const UNITS: [(u64, &str); 6] = [
        (365 * 86_400, "year"),
        (30 * 86_400, "month"),
        (86_400, "day"),
        (3_600, "hour"),
        (60, "minute"),
        (1, "second"),
    ];
    let (size, name) = UNITS
        .iter()
        .copied()
        .find(|(size, _)| magnitude >= *size)
        .unwrap_or((1, "second"));
    let count = magnitude / size;
    let plural = if count == 1 { "" } else { "s" };
    if delta < 0 {
        format!("{count} {name}{plural} ago")
    } else {
        format!("in {count} {name}{plural}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn utc(seconds: i64) -> DateTime<Utc> {
        DateTime::from_timestamp(seconds, 0).unwrap()
    }

    #[test]
    fn auto_detects_seconds_and_millis() {
        let (date, unit) = parse_timestamp("1700000000", Unit::Auto).unwrap();
        assert_eq!(unit, Unit::Seconds);
        assert_eq!(date.to_rfc3339(), "2023-11-14T22:13:20+00:00");
        let (date, unit) = parse_timestamp("1700000000123", Unit::Auto).unwrap();
        assert_eq!(unit, Unit::Milliseconds);
        assert_eq!(date.timestamp_millis(), 1_700_000_000_123);
    }

    #[test]
    fn explicit_unit_and_separators() {
        let (date, _) = parse_timestamp("1_700_000_000", Unit::Milliseconds).unwrap();
        assert_eq!(date.timestamp(), 1_700_000);
        let (date, _) = parse_timestamp(" 0 ", Unit::Seconds).unwrap();
        assert_eq!(date.timestamp(), 0);
        let (date, _) = parse_timestamp("-86400", Unit::Auto).unwrap();
        assert_eq!(date.to_rfc3339(), "1969-12-31T00:00:00+00:00");
    }

    #[test]
    fn rejects_bad_timestamps() {
        assert!(parse_timestamp("", Unit::Auto).is_err());
        assert!(parse_timestamp("12.5", Unit::Auto).is_err());
        assert!(parse_timestamp("abc", Unit::Auto).is_err());
        assert!(parse_timestamp("99999999999999999", Unit::Seconds).is_err());
    }

    #[test]
    fn parses_dates_in_zone() {
        let plus2 = FixedOffset::east_opt(2 * 3600).unwrap();
        assert_eq!(
            parse_date("2024-05-01T12:00:00Z", &plus2)
                .unwrap()
                .timestamp(),
            1_714_564_800
        );
        assert_eq!(
            parse_date("2024-05-01T14:00:00+02:00", &Utc)
                .unwrap()
                .timestamp(),
            1_714_564_800
        );
        assert_eq!(
            parse_date("2024-05-01 14:00:00", &plus2)
                .unwrap()
                .timestamp(),
            1_714_564_800
        );
        assert_eq!(
            parse_date("2024-05-01 12:00", &Utc).unwrap().timestamp(),
            1_714_564_800
        );
        assert_eq!(
            parse_date("2024-05-01", &Utc).unwrap().timestamp(),
            1_714_521_600
        );
        assert_eq!(
            parse_date("Wed, 01 May 2024 12:00:00 +0000", &Utc)
                .unwrap()
                .timestamp(),
            1_714_564_800
        );
    }

    #[test]
    fn rejects_bad_dates() {
        assert!(parse_date("", &Utc).is_err());
        assert!(parse_date("yesterday", &Utc).is_err());
        assert!(parse_date("2024-13-01", &Utc).is_err());
    }

    #[test]
    fn describes_in_utc_and_zone() {
        let plus2 = FixedOffset::east_opt(2 * 3600).unwrap();
        let date = DateTime::from_timestamp_millis(1_714_564_800_250).unwrap();
        let info = describe(date, &plus2, date);
        assert_eq!(info.seconds, 1_714_564_800);
        assert_eq!(info.millis, 1_714_564_800_250);
        assert_eq!(info.utc, "2024-05-01 12:00:00.250 UTC");
        assert_eq!(info.local, "2024-05-01 14:00:00.250 +02:00 (Wed)");
        assert_eq!(info.iso, "2024-05-01T12:00:00.250Z");
        assert_eq!(info.relative, "now");
    }

    #[test]
    fn parse_any_picks_timestamp_or_date() {
        let (date, unit) = parse_any("1700000000", Unit::Auto, &Utc).unwrap();
        assert_eq!(
            (date.timestamp(), unit),
            (1_700_000_000, Some(Unit::Seconds))
        );
        let (date, unit) = parse_any("-1", Unit::Auto, &Utc).unwrap();
        assert_eq!((date.timestamp(), unit), (-1, Some(Unit::Seconds)));
        let (date, unit) = parse_any("2024-05-01", Unit::Auto, &Utc).unwrap();
        assert_eq!((date.timestamp(), unit), (1_714_521_600, None));
        assert!(parse_any("", Unit::Auto, &Utc).is_err());
        assert!(parse_any("-", Unit::Auto, &Utc).is_err());
    }

    #[test]
    fn relative_wording() {
        let now = utc(1_000_000_000);
        assert_eq!(relative(utc(1_000_000_002), now), "now");
        assert_eq!(relative(utc(1_000_000_030), now), "in 30 seconds");
        assert_eq!(relative(utc(1_000_000_000 - 60), now), "1 minute ago");
        assert_eq!(relative(utc(1_000_000_000 - 3 * 86_400), now), "3 days ago");
        assert_eq!(
            relative(utc(1_000_000_000 + 2 * 365 * 86_400), now),
            "in 2 years"
        );
    }
}

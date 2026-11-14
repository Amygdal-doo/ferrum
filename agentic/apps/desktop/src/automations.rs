//! Automations: saved prompts that start a task in their project, by hand
//! or on a schedule while the app is open. Kept in
//! `~/.ferrum/agentic/automations.json`.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, NaiveTime, TimeZone, Utc};
use ferrumagent_ui::core::Mode;
use serde::{Deserialize, Serialize};

/// When an automation runs by itself.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Schedule {
    /// Only with "Run now".
    #[default]
    Manual,
    /// Every `hours` hours after the last run (or creation).
    Every { hours: u32 },
    /// Once a day at this local time.
    Daily { hour: u32, minute: u32 },
}

impl Schedule {
    /// `manual`, `every 4h` / `every 4 hours`, `daily 09:30` /
    /// `daily at 9:30`; `None` when it can't be read.
    #[must_use]
    pub fn parse(text: &str) -> Option<Schedule> {
        let text = text.trim().to_lowercase();
        let words: Vec<&str> = text
            .split_whitespace()
            .filter(|w| *w != "at" && *w != "hours" && *w != "hour")
            .collect();
        match words.as_slice() {
            [] | ["manual"] => Some(Schedule::Manual),
            ["hourly"] => Some(Schedule::Every { hours: 1 }),
            ["every", n] => {
                let hours: u32 = n.trim_end_matches('h').parse().ok()?;
                (1..=24 * 7)
                    .contains(&hours)
                    .then_some(Schedule::Every { hours })
            }
            ["daily", time] => {
                let (h, m) = time.split_once(':').unwrap_or((time, "0"));
                let (hour, minute) = (h.parse().ok()?, m.parse().ok()?);
                (hour < 24 && minute < 60).then_some(Schedule::Daily { hour, minute })
            }
            _ => None,
        }
    }

    /// "Manual", "Every 4 hours", "Daily at 09:30".
    #[must_use]
    pub fn label(self) -> String {
        match self {
            Schedule::Manual => "Manual".into(),
            Schedule::Every { hours: 1 } => "Every hour".into(),
            Schedule::Every { hours } => format!("Every {hours} hours"),
            Schedule::Daily { hour, minute } => format!("Daily at {hour:02}:{minute:02}"),
        }
    }

    /// The first run strictly after `since`, in `tz` (`None`: manual).
    #[must_use]
    pub fn next_after<Tz: TimeZone>(self, since: DateTime<Tz>) -> Option<DateTime<Tz>> {
        match self {
            Schedule::Manual => None,
            Schedule::Every { hours } => Some(since + Duration::hours(i64::from(hours.max(1)))),
            Schedule::Daily { hour, minute } => {
                let at = NaiveTime::from_hms_opt(hour, minute, 0)?;
                let tz = since.timezone();
                let mut day = since.date_naive();
                // Today's time if still ahead, else the next day's (a time
                // skipped by a DST change moves to the following day).
                for _ in 0..3 {
                    if let Some(next) = tz.from_local_datetime(&day.and_time(at)).earliest()
                        && next > since
                    {
                        return Some(next);
                    }
                    day = day.succ_opt()?;
                }
                None
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub project: PathBuf,
    pub prompt: String,
    pub mode: Mode,
    #[serde(default)]
    pub schedule: Schedule,
    #[serde(default = "enabled")]
    pub enabled: bool,
    pub created: DateTime<Utc>,
    #[serde(default)]
    pub last_run: Option<DateTime<Utc>>,
}

fn enabled() -> bool {
    true
}

impl Automation {
    #[must_use]
    pub fn new(name: &str, project: &Path, prompt: &str, mode: Mode, schedule: Schedule) -> Self {
        let created = Utc::now();
        Self {
            id: format!("auto-{}", created.timestamp_micros()),
            name: name.trim().to_string(),
            project: project.to_path_buf(),
            prompt: prompt.trim().to_string(),
            mode,
            schedule,
            enabled: true,
            created,
            last_run: None,
        }
    }

    /// The next scheduled run, in `tz` (`None`: manual or disabled).
    #[must_use]
    pub fn next_run<Tz: TimeZone>(&self, tz: &Tz) -> Option<DateTime<Tz>> {
        if !self.enabled {
            return None;
        }
        let since = self.last_run.unwrap_or(self.created).with_timezone(tz);
        self.schedule.next_after(since)
    }

    /// Whether the schedule says it should run at `now`.
    #[must_use]
    pub fn is_due<Tz: TimeZone>(&self, now: &DateTime<Tz>) -> bool {
        self.next_run(&now.timezone())
            .is_some_and(|next| next <= *now)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct AutomationsFile {
    #[serde(default)]
    automations: Vec<Automation>,
}

/// The automations saved in `file` (missing or broken: none).
#[must_use]
pub fn load(file: &Path) -> Vec<Automation> {
    ferrum_ui::prefs::read::<AutomationsFile>(file).automations
}

pub fn save(file: &Path, automations: &[Automation]) -> std::io::Result<()> {
    ferrum_ui::prefs::write(
        file,
        &AutomationsFile {
            automations: automations.to_vec(),
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    fn at(tz: &FixedOffset, y: i32, mo: u32, d: u32, h: u32, mi: u32) -> DateTime<FixedOffset> {
        tz.with_ymd_and_hms(y, mo, d, h, mi, 0)
            .single()
            .expect("time")
    }

    #[test]
    fn schedules_parse_and_label() {
        assert_eq!(Schedule::parse(""), Some(Schedule::Manual));
        assert_eq!(Schedule::parse("Manual"), Some(Schedule::Manual));
        assert_eq!(
            Schedule::parse("every 4h"),
            Some(Schedule::Every { hours: 4 })
        );
        assert_eq!(
            Schedule::parse("every 6 hours"),
            Some(Schedule::Every { hours: 6 })
        );
        assert_eq!(
            Schedule::parse("daily at 9:05"),
            Some(Schedule::Daily { hour: 9, minute: 5 })
        );
        assert_eq!(
            Schedule::parse("daily 18"),
            Some(Schedule::Daily {
                hour: 18,
                minute: 0
            })
        );
        for bad in ["daily 24:00", "daily 9:60", "every 0h", "weekly", "every x"] {
            assert_eq!(Schedule::parse(bad), None, "{bad}");
        }
        assert_eq!(Schedule::Every { hours: 1 }.label(), "Every hour");
        assert_eq!(
            Schedule::Daily { hour: 7, minute: 3 }.label(),
            "Daily at 07:03"
        );
    }

    #[test]
    fn next_runs_follow_the_schedule() {
        let tz = FixedOffset::east_opt(3600).expect("offset");
        let since = at(&tz, 2026, 11, 14, 10, 30);
        assert_eq!(Schedule::Manual.next_after(since), None);
        assert_eq!(
            Schedule::Every { hours: 4 }.next_after(since),
            Some(at(&tz, 2026, 11, 14, 14, 30))
        );
        // Later today, else tomorrow; exactly now counts as done.
        let daily = |hour, minute| Schedule::Daily { hour, minute }.next_after(since);
        assert_eq!(daily(18, 0), Some(at(&tz, 2026, 11, 14, 18, 0)));
        assert_eq!(daily(9, 0), Some(at(&tz, 2026, 11, 15, 9, 0)));
        assert_eq!(daily(10, 30), Some(at(&tz, 2026, 11, 15, 10, 30)));
        // Month end.
        let last = at(&tz, 2026, 11, 30, 23, 0);
        assert_eq!(
            Schedule::Daily { hour: 8, minute: 0 }.next_after(last),
            Some(at(&tz, 2026, 12, 1, 8, 0))
        );
    }

    #[test]
    fn automations_are_due_after_their_last_run() {
        let tz = FixedOffset::east_opt(0).expect("offset");
        let mut a = Automation::new(
            " Triage ",
            Path::new("/p"),
            "check CI",
            Mode::Ask,
            Schedule::Daily { hour: 9, minute: 0 },
        );
        assert_eq!(a.name, "Triage");
        a.created = at(&tz, 2026, 11, 14, 8, 0).with_timezone(&Utc);
        assert!(!a.is_due(&at(&tz, 2026, 11, 14, 8, 59)));
        assert!(a.is_due(&at(&tz, 2026, 11, 14, 9, 0)));
        a.last_run = Some(at(&tz, 2026, 11, 14, 9, 1).with_timezone(&Utc));
        assert!(!a.is_due(&at(&tz, 2026, 11, 14, 20, 0)));
        assert!(a.is_due(&at(&tz, 2026, 11, 15, 9, 0)));
        a.enabled = false;
        assert!(!a.is_due(&at(&tz, 2026, 11, 16, 9, 0)));
        a.enabled = true;
        a.schedule = Schedule::Manual;
        assert!(!a.is_due(&at(&tz, 2027, 1, 1, 0, 0)));
    }

    #[test]
    fn automations_round_trip_through_json() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("agentic").join("automations.json");
        assert!(load(&file).is_empty());
        let mut a = Automation::new(
            "Nightly",
            Path::new("/code/app"),
            "Update deps",
            Mode::Agent,
            Schedule::Every { hours: 12 },
        );
        a.last_run = Some(Utc::now());
        let b = Automation::new(
            "Review",
            Path::new("/code/b"),
            "Review",
            Mode::Plan,
            Schedule::Manual,
        );
        save(&file, &[a.clone(), b.clone()]).expect("save");
        assert_eq!(load(&file), [a, b]);
        std::fs::write(&file, "{broken").expect("write");
        assert!(load(&file).is_empty());
    }
}

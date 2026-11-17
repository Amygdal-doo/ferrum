//! Port history: which process held which port, and when. Each scan of
//! this Mac is compared with the open entries: a listener seen for the
//! first time opens an entry, one that is gone closes it. Kept in
//! `~/.ferrum/ports/history.json` (chosen by the app), at most
//! [`MAX_ENTRIES`] entries and [`MAX_AGE_DAYS`] days.
//!
//! Only what happens while FerrumPorts runs is recorded; entries still
//! open when it quits are closed at the last time they were seen.

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};

use crate::model::{Listener, Protocol};

pub const MAX_ENTRIES: usize = 1000;
pub const MAX_AGE_DAYS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HistoryEntry {
    pub port: u16,
    pub udp: bool,
    pub pid: Option<u32>,
    pub process: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project: Option<PathBuf>,
    pub first_seen: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    /// Still listening at the last scan.
    pub open: bool,
}

impl HistoryEntry {
    fn of(listener: &Listener, now: DateTime<Utc>) -> Self {
        Self {
            port: listener.port,
            udp: listener.protocol == Protocol::Udp,
            pid: listener.pid,
            process: listener.process.clone(),
            command: listener.command.clone(),
            project: listener.project.clone(),
            first_seen: now,
            last_seen: now,
            open: true,
        }
    }

    fn is(&self, listener: &Listener) -> bool {
        self.port == listener.port
            && self.udp == (listener.protocol == Protocol::Udp)
            && self.pid == listener.pid
            && self.process == listener.process
    }

    #[must_use]
    pub fn protocol(&self) -> Protocol {
        if self.udp {
            Protocol::Udp
        } else {
            Protocol::Tcp
        }
    }

    /// The project's folder name.
    #[must_use]
    pub fn project_name(&self) -> Option<String> {
        let project = self.project.as_ref()?;
        Some(project.file_name().map_or_else(
            || project.display().to_string(),
            |n| n.to_string_lossy().into_owned(),
        ))
    }

    /// How long it listened, as far as FerrumPorts saw: `4 min`, `2 h 5 min`.
    #[must_use]
    pub fn duration(&self) -> String {
        let minutes = (self.last_seen - self.first_seen).num_minutes();
        match minutes {
            0 => "< 1 min".to_string(),
            m if m < 60 => format!("{m} min"),
            m if m < 60 * 24 => format!("{} h {} min", m / 60, m % 60),
            m => format!("{} d {} h", m / (60 * 24), m / 60 % 24),
        }
    }

    /// Whether every word of `query` matches the port (by prefix), pid,
    /// process, command or project.
    #[must_use]
    pub fn matches(&self, query: &str) -> bool {
        let port = self.port.to_string();
        let haystack = format!(
            "{} {} {}",
            self.process,
            self.command.as_deref().unwrap_or_default(),
            self.project_name().unwrap_or_default()
        )
        .to_lowercase();
        query.split_whitespace().all(|word| {
            let word = word.trim_start_matches(':').to_lowercase();
            if word.chars().all(|c| c.is_ascii_digit()) {
                return port.starts_with(&word) || self.pid.is_some_and(|p| p.to_string() == word);
            }
            haystack.contains(&word)
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PortHistory {
    /// Oldest first.
    entries: Vec<HistoryEntry>,
}

impl PortHistory {
    /// Reads the history; a missing file is empty, a corrupt one an error.
    /// Entries left open by the last run are closed.
    pub fn load(path: &Path) -> Result<Self, String> {
        let mut history: Self = match std::fs::read_to_string(path) {
            Ok(text) => serde_json::from_str(&text)
                .map_err(|e| format!("{} is not valid: {e}", path.display()))?,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => return Err(format!("Could not read {}: {e}", path.display())),
        };
        history.close_all();
        Ok(history)
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        let fail = |e: std::io::Error| format!("Could not write {}: {e}", path.display());
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir).map_err(fail)?;
        }
        let text = serde_json::to_string(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        std::fs::write(&tmp, text).map_err(fail)?;
        std::fs::rename(&tmp, path).map_err(fail)
    }

    /// Newest first.
    pub fn entries(&self) -> impl Iterator<Item = &HistoryEntry> {
        self.entries.iter().rev()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Records a scan taken at `now`. Returns whether an entry opened or
    /// closed (worth saving now; `last_seen` updates are not).
    pub fn observe(&mut self, listeners: &[Listener], now: DateTime<Utc>) -> bool {
        let mut changed = false;
        for entry in self.entries.iter_mut().filter(|e| e.open) {
            if listeners.iter().any(|l| entry.is(l)) {
                entry.last_seen = now;
            } else {
                entry.open = false;
                changed = true;
            }
        }
        for listener in listeners {
            if !self.entries.iter().any(|e| e.open && e.is(listener)) {
                self.entries.push(HistoryEntry::of(listener, now));
                changed = true;
            }
        }
        changed | self.prune(now)
    }

    /// Closes every open entry (the app quits, or a new run starts).
    pub fn close_all(&mut self) {
        for entry in &mut self.entries {
            entry.open = false;
        }
    }

    pub fn clear(&mut self) {
        self.entries.clear();
    }

    /// Drops entries last seen more than [`MAX_AGE_DAYS`] ago, then the
    /// oldest beyond [`MAX_ENTRIES`]. Returns whether any went.
    fn prune(&mut self, now: DateTime<Utc>) -> bool {
        let before = self.entries.len();
        let cutoff = now - Duration::days(MAX_AGE_DAYS);
        self.entries.retain(|e| e.open || e.last_seen >= cutoff);
        if self.entries.len() > MAX_ENTRIES {
            let excess = self.entries.len() - MAX_ENTRIES;
            self.entries.drain(..excess);
        }
        self.entries.len() != before
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn listener(port: u16, pid: u32, process: &str) -> Listener {
        Listener {
            protocol: Protocol::Tcp,
            port,
            addresses: vec!["127.0.0.1".into()],
            pid: Some(pid),
            process: process.into(),
            user: None,
            command: Some(format!("{process} --port {port}")),
            started: None,
            cwd: None,
            project: Some(PathBuf::from("/code/shop")),
            container: None,
        }
    }

    fn at(minutes: i64) -> DateTime<Utc> {
        DateTime::<Utc>::from_timestamp(1_800_000_000, 0).expect("time")
            + Duration::minutes(minutes)
    }

    #[test]
    fn opens_updates_and_closes_entries() {
        let mut history = PortHistory::default();
        let node = listener(3000, 10, "node");
        let pg = listener(5432, 20, "postgres");
        assert!(history.observe(&[node.clone(), pg.clone()], at(0)));
        // Same listeners again: only last_seen moves.
        assert!(!history.observe(&[node.clone(), pg.clone()], at(5)));
        // node stops; a new node (other pid) takes 3000 later.
        assert!(history.observe(std::slice::from_ref(&pg), at(10)));
        assert!(history.observe(&[pg.clone(), listener(3000, 11, "node")], at(12)));
        let entries: Vec<_> = history.entries().collect();
        assert_eq!(entries.len(), 3);
        assert_eq!(
            (entries[0].port, entries[0].pid, entries[0].open),
            (3000, Some(11), true)
        );
        let old_node = entries
            .iter()
            .find(|e| e.pid == Some(10))
            .expect("old node");
        assert!(!old_node.open);
        assert_eq!(old_node.last_seen, at(5));
        assert_eq!(old_node.duration(), "5 min");
        assert_eq!(old_node.project_name().as_deref(), Some("shop"));
    }

    #[test]
    fn keeps_a_limited_history() {
        let mut history = PortHistory::default();
        history.observe(&[listener(1000, 1, "old")], at(0));
        history.observe(&[], at(1));
        // Forty days later the closed entry is gone.
        history.observe(&[listener(2000, 2, "new")], at(60 * 24 * 40));
        assert_eq!(history.len(), 1);
        for i in 0..(MAX_ENTRIES as u32 + 10) {
            history.observe(
                &[listener(3000, 100 + i, "x")],
                at(60 * 24 * 40 + i64::from(i)),
            );
        }
        assert_eq!(history.len(), MAX_ENTRIES);
    }

    #[test]
    fn searches_and_formats() {
        let mut history = PortHistory::default();
        history.observe(&[listener(5432, 20, "postgres")], at(0));
        history.observe(&[listener(5432, 20, "postgres")], at(125));
        let entry = history.entries().next().expect("entry").clone();
        assert!(entry.matches("54"));
        assert!(entry.matches("20"));
        assert!(entry.matches("postgres shop"));
        assert!(!entry.matches("node"));
        assert!(!entry.matches("2"));
        assert_eq!(entry.duration(), "2 h 5 min");
    }

    #[test]
    fn round_trips_and_closes_on_load() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("ports").join("history.json");
        assert!(PortHistory::load(&path).expect("load").is_empty());
        let mut history = PortHistory::default();
        history.observe(&[listener(8080, 7, "python")], at(0));
        history.save(&path).expect("save");
        let loaded = PortHistory::load(&path).expect("load");
        assert_eq!(loaded.len(), 1);
        assert!(!loaded.entries().next().expect("entry").open);
        std::fs::write(&path, "[").expect("write");
        assert!(PortHistory::load(&path).is_err());
    }
}

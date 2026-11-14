//! FerrumChat's saved threads across projects: the sidebar's folders and
//! recent conversations, the tasks list, search and usage. Read from
//! FerrumChat's thread store (off the UI thread; files are whole threads).

use std::path::{Path, PathBuf};

use chrono::{DateTime, Duration, Utc};
use ferrumagent_ui::core::Mode;
use ferrumagent_ui::core::store::{ThreadStore, ThreadSummary};

/// A thread and the project it belongs to.
#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub project: PathBuf,
    pub thread: ThreadSummary,
}

/// The threads of some projects, each newest first.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct ThreadIndex {
    pub projects: Vec<(PathBuf, Vec<ThreadSummary>)>,
}

impl ThreadIndex {
    /// Reads the threads of `projects` from FerrumChat's store in `home`.
    #[must_use]
    pub fn load(home: &Path, projects: &[PathBuf]) -> Self {
        let projects = projects
            .iter()
            .map(|p| (p.clone(), ThreadStore::new(home, p).list()))
            .collect();
        Self { projects }
    }

    #[must_use]
    pub fn of(&self, project: &Path) -> &[ThreadSummary] {
        self.projects
            .iter()
            .find(|(p, _)| p == project)
            .map_or(&[], |(_, t)| t.as_slice())
    }

    /// Every thread, newest first.
    #[must_use]
    pub fn all(&self) -> Vec<Entry> {
        let mut all: Vec<Entry> = self
            .projects
            .iter()
            .flat_map(|(project, threads)| {
                threads.iter().map(|t| Entry {
                    project: project.clone(),
                    thread: t.clone(),
                })
            })
            .collect();
        all.sort_by_key(|e| std::cmp::Reverse(e.thread.updated));
        all
    }

    /// The `n` newest threads that have messages.
    #[must_use]
    pub fn recent(&self, n: usize) -> Vec<Entry> {
        let mut all = self.all();
        all.retain(|e| e.thread.messages > 0);
        all.truncate(n);
        all
    }

    /// Threads in `mode` (Ask-mode threads are the discussions).
    #[must_use]
    pub fn with_mode(&self, mode: Mode) -> Vec<Entry> {
        let mut all = self.all();
        all.retain(|e| e.thread.mode == mode && e.thread.messages > 0);
        all
    }

    /// Token totals, all time and in the 7 days before `now`.
    #[must_use]
    pub fn usage(&self, now: DateTime<Utc>) -> Usage {
        let week_start = now - Duration::days(7);
        let mut usage = Usage::default();
        for (project, threads) in &self.projects {
            let mut row = UsageRow {
                project: project.clone(),
                ..UsageRow::default()
            };
            for t in threads {
                let tokens = tokens(t);
                row.total += tokens;
                if t.updated >= week_start {
                    row.week += tokens;
                }
            }
            usage.total += row.total;
            usage.week += row.week;
            if row.total > 0 {
                usage.projects.push(row);
            }
        }
        usage.projects.sort_by_key(|r| std::cmp::Reverse(r.total));
        let mut threads = self.all();
        threads.retain(|e| tokens(&e.thread) > 0);
        threads.sort_by_key(|e| std::cmp::Reverse(tokens(&e.thread)));
        threads.truncate(10);
        usage.threads = threads;
        usage
    }
}

/// Tokens a thread used (input and output).
#[must_use]
pub fn tokens(thread: &ThreadSummary) -> u64 {
    thread.tokens
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct UsageRow {
    pub project: PathBuf,
    pub total: u64,
    pub week: u64,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Usage {
    pub total: u64,
    pub week: u64,
    /// Projects with any usage, biggest first.
    pub projects: Vec<UsageRow>,
    /// The ten biggest threads.
    pub threads: Vec<Entry>,
}

/// `12.3k`, `4.1M`.
#[must_use]
pub fn format_tokens(n: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let f = n as f64;
    match n {
        0..1_000 => n.to_string(),
        1_000..1_000_000 => format!("{:.1}k", f / 1e3),
        _ => format!("{:.1}M", f / 1e6),
    }
}

/// "now", "5m", "3h", "2d", "6w".
#[must_use]
pub fn ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (now - then).num_minutes().max(0);
    match minutes {
        0 => "now".into(),
        1..60 => format!("{minutes}m"),
        60..1440 => format!("{}h", minutes / 60),
        1440..20160 => format!("{}d", minutes / 1440),
        _ => format!("{}w", minutes / 10080),
    }
}

/// A search result.
#[derive(Debug, Clone, PartialEq)]
pub struct Hit {
    pub entry: Entry,
    /// The matching message text around the match (empty for a title hit).
    pub snippet: String,
}

/// Whether every word of `query` is in `text` (case-insensitive).
#[must_use]
pub fn matches(query: &str, text: &str) -> bool {
    let text = text.to_lowercase();
    let mut words = query.split_whitespace().peekable();
    words.peek().is_some() && words.all(|w| text.contains(&w.to_lowercase()))
}

/// About `width` characters of `text` around the first word of `query`.
#[must_use]
pub fn snippet(text: &str, query: &str, width: usize) -> Option<String> {
    let lower = text.to_lowercase();
    let first = query.split_whitespace().next()?.to_lowercase();
    let at = lower.find(&first)?;
    // Lowercasing can change byte lengths: map back by character count.
    let char_at = lower[..at].chars().count();
    let chars: Vec<char> = text.chars().collect();
    let start = char_at.saturating_sub(width / 3);
    let end = (start + width).min(chars.len());
    let mut out: String = chars[start..end].iter().collect();
    out = out.split_whitespace().collect::<Vec<_>>().join(" ");
    if start > 0 {
        out.insert(0, '…');
    }
    if end < chars.len() {
        out.push('…');
    }
    Some(out)
}

/// Threads of `projects` whose title or messages contain every word of
/// `query`, newest first, at most `limit`.
#[must_use]
pub fn search(home: &Path, index: &ThreadIndex, query: &str, limit: usize) -> Vec<Hit> {
    if query.split_whitespace().next().is_none() {
        return Vec::new();
    }
    let mut hits = Vec::new();
    for entry in index.all() {
        if hits.len() >= limit {
            break;
        }
        if matches(query, &entry.thread.title) {
            hits.push(Hit {
                entry,
                snippet: String::new(),
            });
            continue;
        }
        let Ok(thread) = ThreadStore::new(home, &entry.project).load(&entry.thread.id) else {
            continue;
        };
        let text: String = thread
            .messages
            .iter()
            .map(|m| m.text())
            .collect::<Vec<_>>()
            .join("\n");
        if matches(query, &text) {
            let snippet = snippet(&text, query, 90).unwrap_or_default();
            hits.push(Hit { entry, snippet });
        }
    }
    hits
}

#[cfg(test)]
#[path = "threads_tests.rs"]
mod tests;

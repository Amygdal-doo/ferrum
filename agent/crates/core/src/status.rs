//! Thread status: what each chat thread needs from the user, as a pure
//! state machine a UI feeds with what its sessions report.
//!
//! | From | Input | To |
//! |---|---|---|
//! | any | turn started | Working |
//! | Working | permission asked | **Needs you** (alert unless viewed) |
//! | Needs you | last open prompt answered | Working |
//! | Working | plan written, then turn ended (Plan mode) | **Needs you** (alert) if not viewed, else Idle |
//! | Working / Needs you | turn ended | **Done** (alert) if not viewed, else Idle |
//! | Working / Needs you | turn failed | **Done** with `error` (alert) if not viewed, else Idle |
//! | Working / Needs you | turn cancelled (the user stopped it) | Idle, no alert |
//! | Working | turn ended with more messages queued | stays Working |
//! | Done / Needs you (plan) | viewed | Idle |
//!
//! "Viewed" means the thread is the one shown, its chat is visible and its
//! window is active. A thread with open permission prompts stays Needs you
//! when viewed: it is still waiting. Alerts fire once per transition: a
//! second prompt while already Needs you does not alert again, and nothing
//! alerts while the thread is viewed (its prompt or result is on screen).
//!
//! An error is not a state of its own: it is Done with [`StatusEntry::error`]
//! set, so the thread lands in the same "come and look" section, drawn in
//! the error colour.

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::permissions::PermissionRequest;
use crate::provider::{Role, StopReason};
use crate::thread::Thread;

/// A thread's status, in the order the thread list shows its sections.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub enum ThreadStatus {
    /// Waiting on the user: a permission prompt, or a plan to build.
    NeedsYou,
    /// A turn finished since the user last looked at the thread.
    Done,
    /// A turn is running.
    Working,
    #[default]
    Idle,
}

impl ThreadStatus {
    /// Section order.
    pub const ORDER: [Self; 4] = [Self::NeedsYou, Self::Done, Self::Working, Self::Idle];

    pub fn label(self) -> &'static str {
        match self {
            Self::NeedsYou => "Needs you",
            Self::Done => "Done",
            Self::Working => "Working",
            Self::Idle => "Idle",
        }
    }
}

/// How a turn ended, as far as the status cares.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TurnOutcome {
    Finished,
    Cancelled,
    Failed,
}

impl TurnOutcome {
    pub fn of(stop: &StopReason) -> Self {
        match stop {
            StopReason::Cancelled => Self::Cancelled,
            StopReason::Error(_) => Self::Failed,
            StopReason::EndTurn
            | StopReason::ToolUse
            | StopReason::MaxTokens
            | StopReason::Refusal => Self::Finished,
        }
    }
}

/// What happened to a thread.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StatusInput {
    TurnStarted,
    /// A permission prompt opened; `summary` is what it asks.
    PermissionAsked {
        summary: String,
    },
    /// A permission prompt was answered (or closed).
    PermissionAnswered,
    /// Plan mode wrote a plan this turn: it waits for "Build".
    PlanWritten,
    /// The turn ended. `more_queued`: another queued message starts next.
    TurnEnded {
        outcome: TurnOutcome,
        summary: String,
        more_queued: bool,
    },
    /// The user is looking at the thread.
    Viewed,
}

/// A transition worth a notification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alert {
    NeedsYou { summary: String },
    Finished { summary: String, error: bool },
}

/// One thread's status and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StatusEntry {
    pub status: ThreadStatus,
    /// The last turn failed (Done drawn in the error colour).
    pub error: bool,
    /// Open permission prompts.
    pub prompts: u32,
    /// A plan written this turn (or not yet looked at) waits for "Build".
    pub plan: bool,
    /// When `status` last changed.
    pub changed: DateTime<Utc>,
    /// One line: the question asked, or how the last turn ended.
    pub summary: String,
}

impl StatusEntry {
    pub fn idle(now: DateTime<Utc>) -> Self {
        Self {
            status: ThreadStatus::Idle,
            error: false,
            prompts: 0,
            plan: false,
            changed: now,
            summary: String::new(),
        }
    }

    fn set(&mut self, status: ThreadStatus, now: DateTime<Utc>) {
        if self.status != status {
            self.status = status;
            self.changed = now;
        }
    }

    /// Applies `input`; `viewing` is whether the user is looking at the
    /// thread right now. Returns the alert to deliver, if any.
    pub fn apply(
        &mut self,
        input: StatusInput,
        viewing: bool,
        now: DateTime<Utc>,
    ) -> Option<Alert> {
        match input {
            StatusInput::TurnStarted => {
                self.error = false;
                self.prompts = 0;
                self.plan = false;
                self.summary.clear();
                self.set(ThreadStatus::Working, now);
                None
            }
            StatusInput::PermissionAsked { summary } => {
                self.prompts += 1;
                if self.status == ThreadStatus::NeedsYou {
                    return None;
                }
                self.summary = summary.clone();
                self.set(ThreadStatus::NeedsYou, now);
                (!viewing).then_some(Alert::NeedsYou { summary })
            }
            StatusInput::PermissionAnswered => {
                // A prompt closed after its turn ended (prompts already
                // reset) changes nothing.
                if self.prompts == 0 {
                    return None;
                }
                self.prompts -= 1;
                if self.prompts == 0 && self.status == ThreadStatus::NeedsYou {
                    self.set(ThreadStatus::Working, now);
                }
                None
            }
            StatusInput::PlanWritten => {
                self.plan = true;
                None
            }
            StatusInput::TurnEnded {
                outcome,
                summary,
                more_queued,
            } => {
                self.prompts = 0;
                match outcome {
                    TurnOutcome::Finished if more_queued => {
                        self.plan = false;
                        self.set(ThreadStatus::Working, now);
                        None
                    }
                    TurnOutcome::Cancelled => {
                        self.plan = false;
                        self.summary = summary;
                        self.set(ThreadStatus::Idle, now);
                        None
                    }
                    _ if viewing => {
                        self.plan = false;
                        self.error = false;
                        self.summary = summary;
                        self.set(ThreadStatus::Idle, now);
                        None
                    }
                    TurnOutcome::Finished if self.plan => {
                        self.summary = format!("Plan ready to build. {summary}")
                            .trim_end()
                            .to_string();
                        self.set(ThreadStatus::NeedsYou, now);
                        Some(Alert::NeedsYou {
                            summary: self.summary.clone(),
                        })
                    }
                    TurnOutcome::Finished | TurnOutcome::Failed => {
                        self.plan = false;
                        self.error = outcome == TurnOutcome::Failed;
                        self.summary = summary.clone();
                        self.set(ThreadStatus::Done, now);
                        Some(Alert::Finished {
                            summary,
                            error: self.error,
                        })
                    }
                }
            }
            StatusInput::Viewed => {
                let waiting_on_plan = self.status == ThreadStatus::NeedsYou && self.prompts == 0;
                if self.status == ThreadStatus::Done || waiting_on_plan {
                    self.error = false;
                    self.plan = false;
                    self.set(ThreadStatus::Idle, now);
                }
                None
            }
        }
    }
}

/// The status of every thread a chat has seen this run (others are Idle).
/// Kept in memory: after a restart every thread starts Idle.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StatusBoard {
    entries: HashMap<String, StatusEntry>,
}

impl StatusBoard {
    pub fn get(&self, thread_id: &str) -> Option<&StatusEntry> {
        self.entries.get(thread_id)
    }

    pub fn status(&self, thread_id: &str) -> ThreadStatus {
        self.get(thread_id).map_or(ThreadStatus::Idle, |e| e.status)
    }

    /// Applies `input` to a thread (Idle if new); see [`StatusEntry::apply`].
    pub fn apply(
        &mut self,
        thread_id: &str,
        input: StatusInput,
        viewing: bool,
        now: DateTime<Utc>,
    ) -> Option<Alert> {
        self.entries
            .entry(thread_id.to_string())
            .or_insert_with(|| StatusEntry::idle(now))
            .apply(input, viewing, now)
    }

    pub fn remove(&mut self, thread_id: &str) {
        self.entries.remove(thread_id);
    }

    /// Threads not Idle.
    pub fn active(&self) -> impl Iterator<Item = (&str, &StatusEntry)> {
        self.entries
            .iter()
            .filter(|(_, e)| e.status != ThreadStatus::Idle)
            .map(|(id, e)| (id.as_str(), e))
    }

    /// Threads that need the user.
    pub fn needs_you(&self) -> usize {
        self.active()
            .filter(|(_, e)| e.status == ThreadStatus::NeedsYou)
            .count()
    }

    /// When a thread's row last changed: its status change, or `fallback`
    /// (the thread's own update time) when that is later or it has none.
    pub fn changed(&self, thread_id: &str, fallback: DateTime<Utc>) -> DateTime<Utc> {
        self.get(thread_id)
            .map_or(fallback, |e| e.changed.max(fallback))
    }

    /// `items` in sections of [`ThreadStatus::ORDER`], newest change
    /// first; empty sections are left out.
    pub fn group<'a, T>(
        &self,
        items: &'a [T],
        id: impl Fn(&T) -> &str,
        updated: impl Fn(&T) -> DateTime<Utc>,
    ) -> Vec<(ThreadStatus, Vec<&'a T>)> {
        ThreadStatus::ORDER
            .into_iter()
            .filter_map(|status| {
                let mut rows: Vec<&T> = items
                    .iter()
                    .filter(|t| self.status(id(t)) == status)
                    .collect();
                rows.sort_by_key(|t| std::cmp::Reverse(self.changed(id(t), updated(t))));
                (!rows.is_empty()).then_some((status, rows))
            })
            .collect()
    }
}

/// `<1m`, `12m`, `3h`, `1d`: time since `then`.
pub fn short_ago(then: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let minutes = (now - then).num_minutes().max(0);
    match minutes {
        0 => "<1m".into(),
        1..60 => format!("{minutes}m"),
        60..1440 => format!("{}h", minutes / 60),
        _ => format!("{}d", minutes / 1440),
    }
}

/// The first non-empty line of `text`, at most `max` characters.
pub fn one_line(text: &str, max: usize) -> String {
    let line = text
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty())
        .unwrap_or_default();
    if line.chars().count() <= max {
        return line.to_string();
    }
    let mut out: String = line.chars().take(max.saturating_sub(1)).collect();
    out.push('…');
    out
}

/// What a permission prompt asks, in one line.
pub fn permission_summary(request: &PermissionRequest) -> String {
    let title = if request.title.trim().is_empty() {
        &request.tool
    } else {
        &request.title
    };
    one_line(&format!("Allow {title}?"), 100)
}

/// How a turn ended, in one line: the error, or the start of the last
/// answer.
pub fn turn_summary(thread: &Thread, stop: &StopReason) -> String {
    match stop {
        StopReason::Error(e) => one_line(&format!("Failed: {e}"), 100),
        StopReason::Cancelled => "Stopped.".into(),
        _ => thread
            .messages
            .iter()
            .rev()
            .filter(|m| m.role == Role::Assistant)
            .map(|m| m.text())
            .find(|t| !t.trim().is_empty())
            .map_or_else(|| "Turn finished.".into(), |t| one_line(&t, 100)),
    }
}

#[cfg(test)]
#[path = "status_tests.rs"]
mod tests;

//! When an agent's status change deserves a notification: it starts
//! needing you (a question or a permission prompt), it fails, or it
//! finishes a turn while its terminal is not on screen. One notification per transition, and
//! never the same kind for the same terminal twice within [`DEBOUNCE`].
//!
//! The FerrumTerm window delivers them through `ferrum_ui::notify` (toast,
//! system notification, sound, as set in Settings › Notifications); while
//! no window is open the server posts the system notifications itself.

use ferrumterm_protocol::{BlockId, ProgramState, ProgramStatus};
use std::collections::HashMap;
use std::time::{Duration, Instant};

/// Minimum time between two notifications of one kind for one terminal.
pub const DEBOUNCE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Alert {
    /// `question`: the line the agent is asking, when known.
    NeedsYou {
        question: Option<String>,
    },
    Finished,
    /// `message`: what went wrong, when known.
    Failed {
        message: Option<String>,
    },
}

impl Alert {
    /// The `ferrum_ui::notify` event (FerrumTerm's `app_info` events).
    #[must_use]
    pub fn event(&self) -> &'static str {
        match self {
            Self::NeedsYou { .. } => "blocked",
            Self::Finished => "done",
            Self::Failed { .. } => "error",
        }
    }

    /// Title and body of the notification for the agent called `name`.
    #[must_use]
    pub fn text(&self, name: &str) -> (String, String) {
        match self {
            Self::NeedsYou { question } => (
                format!("{name} needs you"),
                question
                    .clone()
                    .unwrap_or_else(|| "Waiting for your answer".into()),
            ),
            Self::Finished => (format!("{name} finished"), "Ready for you".into()),
            Self::Failed { message } => (
                format!("{name} failed"),
                message
                    .clone()
                    .unwrap_or_else(|| "The agent stopped with an error".into()),
            ),
        }
    }
}

#[derive(Debug, Default)]
pub struct Alerts {
    /// Last state seen per terminal.
    states: HashMap<BlockId, ProgramState>,
    /// Last notification per terminal and kind.
    sent: HashMap<(BlockId, &'static str), Instant>,
}

impl Alerts {
    /// Records `block`'s new status; returns the notification it deserves.
    /// `visible`: the terminal is on screen in the active window.
    pub fn on_status(
        &mut self,
        block: BlockId,
        status: Option<&ProgramStatus>,
        visible: bool,
        now: Instant,
    ) -> Option<Alert> {
        let previous = match status {
            Some(s) => self.states.insert(block, s.state),
            None => self.states.remove(&block),
        };
        let status = status?;
        if previous == Some(status.state) {
            return None;
        }
        let alert = match status.state {
            ProgramState::Blocked => Alert::NeedsYou {
                question: status.message.clone().filter(|m| !m.trim().is_empty()),
            },
            ProgramState::Error => Alert::Failed {
                message: status.message.clone().filter(|m| !m.trim().is_empty()),
            },
            ProgramState::Done if previous == Some(ProgramState::Working) && !visible => {
                Alert::Finished
            }
            _ => return None,
        };
        let key = (block, alert.event());
        if self
            .sent
            .get(&key)
            .is_some_and(|at| now.saturating_duration_since(*at) < DEBOUNCE)
        {
            return None;
        }
        self.sent.insert(key, now);
        Some(alert)
    }

    /// Forgets terminals that are gone.
    pub fn retain(&mut self, alive: impl Fn(BlockId) -> bool) {
        self.states.retain(|b, _| alive(*b));
        self.sent.retain(|(b, _), _| alive(*b));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn status(state: ProgramState, message: Option<&str>) -> ProgramStatus {
        ProgramStatus {
            message: message.map(str::to_owned),
            ..ProgramStatus::new(state)
        }
    }

    #[test]
    fn needs_you_once_per_transition() {
        let mut a = Alerts::default();
        let b = BlockId(1);
        let t = Instant::now();
        assert_eq!(
            a.on_status(b, Some(&status(ProgramState::Working, None)), false, t),
            None
        );
        let blocked = status(ProgramState::Blocked, Some("Do you want to proceed?"));
        assert_eq!(
            a.on_status(b, Some(&blocked), true, t),
            Some(Alert::NeedsYou {
                question: Some("Do you want to proceed?".into())
            })
        );
        // The same state again (a new message, a re-sent status) is quiet.
        assert_eq!(a.on_status(b, Some(&blocked), false, t), None);
    }

    #[test]
    fn finished_only_after_working_and_out_of_sight() {
        let mut a = Alerts::default();
        let b = BlockId(2);
        let t = Instant::now();
        let working = status(ProgramState::Working, None);
        let done = status(ProgramState::Done, None);
        a.on_status(b, Some(&working), false, t);
        assert_eq!(a.on_status(b, Some(&done), true, t), None, "on screen");
        a.on_status(b, Some(&working), false, t);
        assert_eq!(
            a.on_status(b, Some(&done), false, t + DEBOUNCE),
            Some(Alert::Finished)
        );
        // Idle → done (e.g. a hook) is not a finished turn we saw start.
        let c = BlockId(3);
        a.on_status(c, Some(&status(ProgramState::Idle, None)), false, t);
        assert_eq!(a.on_status(c, Some(&done), false, t), None);
    }

    #[test]
    fn debounces_flapping_agents() {
        let mut a = Alerts::default();
        let b = BlockId(4);
        let t = Instant::now();
        let working = status(ProgramState::Working, None);
        let done = status(ProgramState::Done, None);
        a.on_status(b, Some(&working), false, t);
        assert!(a.on_status(b, Some(&done), false, t).is_some());
        a.on_status(b, Some(&working), false, t);
        let soon = t + Duration::from_secs(2);
        assert_eq!(a.on_status(b, Some(&done), false, soon), None);
        a.on_status(b, Some(&working), false, soon);
        assert!(a.on_status(b, Some(&done), false, t + DEBOUNCE).is_some());
        // Clearing the status and forgetting the block.
        assert_eq!(a.on_status(b, None, false, t), None);
        a.retain(|_| false);
        assert!(a.states.is_empty() && a.sent.is_empty());
    }

    #[test]
    fn notification_text() {
        let (title, body) = Alert::NeedsYou { question: None }.text("claude");
        assert_eq!(title, "claude needs you");
        assert!(!body.is_empty());
        assert_eq!(Alert::Finished.text("codex").0, "codex finished");
        assert_eq!(Alert::Finished.event(), "done");
        let failed = Alert::Failed { message: None };
        assert_eq!(failed.event(), "error");
        assert_eq!(failed.text("codex").0, "codex failed");
    }

    #[test]
    fn failed_agents_alert_even_on_screen() {
        let mut a = Alerts::default();
        let b = BlockId(5);
        let t = Instant::now();
        a.on_status(b, Some(&status(ProgramState::Working, None)), true, t);
        assert_eq!(
            a.on_status(
                b,
                Some(&status(ProgramState::Error, Some("rate limited"))),
                true,
                t
            ),
            Some(Alert::Failed {
                message: Some("rate limited".into())
            })
        );
        // Debounced per kind: a needs-you right after still comes through.
        let blocked = status(ProgramState::Blocked, None);
        assert!(a.on_status(b, Some(&blocked), true, t).is_some());
        let error = status(ProgramState::Error, None);
        assert_eq!(a.on_status(b, Some(&error), true, t), None);
    }
}

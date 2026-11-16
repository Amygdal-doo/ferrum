//! Interactive rebase: the commits after a base, what to do with each, and
//! the todo list git runs. Steps are oldest first, like git's todo.

use crate::commit::Oid;

/// What happens to one commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum RebaseAction {
    #[default]
    Pick,
    /// Pick with a new message.
    Reword,
    /// Pick, then stop so the commit can be amended.
    Edit,
    /// Meld into the previous commit, keeping both messages.
    Squash,
    /// Meld into the previous commit, keeping only its message.
    Fixup,
    Drop,
}

impl RebaseAction {
    pub const ALL: [Self; 6] = [
        Self::Pick,
        Self::Reword,
        Self::Edit,
        Self::Squash,
        Self::Fixup,
        Self::Drop,
    ];

    /// git's todo command.
    #[must_use]
    pub const fn command(self) -> &'static str {
        match self {
            Self::Pick | Self::Reword => "pick",
            Self::Edit => "edit",
            Self::Squash => "squash",
            Self::Fixup => "fixup",
            Self::Drop => "drop",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pick => "Pick",
            Self::Reword => "Reword",
            Self::Edit => "Edit",
            Self::Squash => "Squash",
            Self::Fixup => "Fixup",
            Self::Drop => "Drop",
        }
    }

    /// Keyboard shortcut in the rebase dialog (git's one-letter commands).
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Pick => "p",
            Self::Reword => "r",
            Self::Edit => "e",
            Self::Squash => "s",
            Self::Fixup => "f",
            Self::Drop => "d",
        }
    }

    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|a| a.key() == key)
    }

    /// Melds into the commit before it.
    #[must_use]
    pub const fn melds(self) -> bool {
        matches!(self, Self::Squash | Self::Fixup)
    }
}

/// One commit of the rebase.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RebaseStep {
    pub oid: Oid,
    /// The original message (subject and body).
    pub message: String,
    pub action: RebaseAction,
    /// New message for [`RebaseAction::Reword`].
    pub reword: Option<String>,
}

impl RebaseStep {
    #[must_use]
    pub fn new(oid: Oid, message: String) -> Self {
        Self {
            oid,
            message,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn summary(&self) -> &str {
        self.message.lines().next().unwrap_or_default()
    }

    /// The message the commit ends up with.
    #[must_use]
    pub fn final_message(&self) -> &str {
        match (&self.action, &self.reword) {
            (RebaseAction::Reword, Some(m)) if !m.trim().is_empty() => m,
            _ => &self.message,
        }
    }
}

/// One commit of the result, for the preview.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreviewCommit {
    /// Summary of the resulting message.
    pub summary: String,
    /// Commits melded into it, as short ids (the first is its own).
    pub sources: Vec<String>,
    /// The rebase stops after it (`edit`).
    pub stops: bool,
    pub reworded: bool,
}

/// Rebase every commit in `base..HEAD` with the chosen actions and order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebasePlan {
    /// The commit the rebased ones are replayed onto (it is not changed).
    pub base: Oid,
    /// Oldest first.
    pub steps: Vec<RebaseStep>,
}

impl RebasePlan {
    #[must_use]
    pub fn new(base: Oid, steps: Vec<RebaseStep>) -> Self {
        Self { base, steps }
    }

    /// Why git would refuse the plan, in words.
    pub fn validate(&self) -> Result<(), String> {
        let first = self.steps.iter().find(|s| s.action != RebaseAction::Drop);
        if let Some(s) = first.filter(|s| s.action.melds()) {
            return Err(format!(
                "{} of {} needs an earlier commit to meld into.",
                s.action.label(),
                s.oid.short()
            ));
        }
        if let Some(s) = self.steps.iter().find(|s| {
            s.action == RebaseAction::Reword
                && s.reword.as_deref().is_none_or(|m| m.trim().is_empty())
        }) {
            return Err(format!("Enter the new message for {}.", s.oid.short()));
        }
        Ok(())
    }

    /// Nothing would change: every commit picked in its original order.
    #[must_use]
    pub fn is_noop(&self, original: &[Oid]) -> bool {
        self.steps.iter().all(|s| s.action == RebaseAction::Pick)
            && self.steps.iter().map(|s| &s.oid).eq(original.iter())
    }

    /// git's todo list. `reword_file(i)` is the file holding step `i`'s new
    /// message (already shell-quoted); rewording runs as `pick` followed by
    /// an `exec` that amends the message, so nothing waits for an editor.
    #[must_use]
    pub fn todo(&self, reword_file: impl Fn(usize) -> String) -> String {
        let mut out = String::new();
        for (i, s) in self.steps.iter().enumerate() {
            out.push_str(s.action.command());
            out.push(' ');
            out.push_str(s.oid.as_str());
            out.push(' ');
            out.push_str(&one_line(s.summary()));
            out.push('\n');
            if s.action == RebaseAction::Reword {
                out.push_str(
                    "exec git commit --amend --allow-empty --no-verify --quiet --cleanup=strip --file=",
                );
                out.push_str(&reword_file(i));
                out.push('\n');
            }
        }
        out
    }

    /// The history the plan produces, oldest first.
    #[must_use]
    pub fn preview(&self) -> Vec<PreviewCommit> {
        let mut out: Vec<PreviewCommit> = Vec::new();
        for s in &self.steps {
            let short = s.oid.short().to_owned();
            match s.action {
                RebaseAction::Drop => {}
                RebaseAction::Squash | RebaseAction::Fixup if !out.is_empty() => {
                    if let Some(last) = out.last_mut() {
                        last.sources.push(short);
                    }
                }
                action => out.push(PreviewCommit {
                    summary: s
                        .final_message()
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .to_owned(),
                    sources: vec![short],
                    stops: action == RebaseAction::Edit,
                    reworded: action == RebaseAction::Reword,
                }),
            }
        }
        out
    }

    /// Moves step `from` to index `to` (both clamped).
    pub fn move_step(&mut self, from: usize, to: usize) {
        if from >= self.steps.len() {
            return;
        }
        let to = to.min(self.steps.len() - 1);
        let step = self.steps.remove(from);
        self.steps.insert(to, step);
    }
}

/// A todo line must stay on one line.
fn one_line(s: &str) -> String {
    s.replace(['\n', '\r'], " ")
}

/// How far a rebase in progress has got (from git's `rebase-merge` state).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct RebaseProgress {
    /// Todo lines done so far (1-based step number).
    pub step: usize,
    pub total: usize,
    /// The todo line it stopped at, e.g. `edit 1a2b3c4 Message`.
    pub current: Option<String>,
    /// It stopped at an `edit` line (not a conflict).
    pub editing: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(oid: &str, msg: &str, action: RebaseAction) -> RebaseStep {
        RebaseStep {
            action,
            ..RebaseStep::new(Oid(oid.repeat(40 / oid.len())), msg.to_owned())
        }
    }

    fn plan(steps: Vec<RebaseStep>) -> RebasePlan {
        RebasePlan::new(Oid("0".repeat(40)), steps)
    }

    #[test]
    fn todo_maps_actions_and_rewords_with_exec() {
        use RebaseAction::*;
        let mut p = plan(vec![
            step("a", "First\n\nbody", Pick),
            step("b", "Second", Reword),
            step("c", "Third", Squash),
            step("d", "Fourth", Fixup),
            step("e", "Fifth", Edit),
            step("f", "Sixth", Drop),
        ]);
        p.steps[1].reword = Some("Second, reworded".into());
        let todo = p.todo(|i| format!("'/tmp/msg {i}'"));
        let lines: Vec<&str> = todo.lines().collect();
        assert_eq!(lines[0], format!("pick {} First", "a".repeat(40)));
        assert_eq!(lines[1], format!("pick {} Second", "b".repeat(40)));
        assert_eq!(
            lines[2],
            "exec git commit --amend --allow-empty --no-verify --quiet --cleanup=strip --file='/tmp/msg 1'"
        );
        assert!(lines[3].starts_with("squash c"));
        assert!(lines[4].starts_with("fixup d"));
        assert!(lines[5].starts_with("edit e"));
        assert!(lines[6].starts_with("drop f"));
        assert_eq!(lines.len(), 7);
        assert!(p.validate().is_ok());
    }

    #[test]
    fn validation_catches_what_git_would_refuse() {
        use RebaseAction::*;
        let p = plan(vec![step("a", "A", Drop), step("b", "B", Squash)]);
        assert!(p.validate().expect_err("invalid").contains("earlier commit"));
        let p = plan(vec![step("a", "A", Reword)]);
        assert!(p.validate().expect_err("invalid").contains("new message"));
        let mut p = plan(vec![step("a", "A", Pick), step("b", "B", Fixup)]);
        assert!(p.validate().is_ok());
        p.move_step(1, 0);
        assert!(p.validate().is_err());
    }

    #[test]
    fn preview_melds_squashes_and_skips_drops() {
        use RebaseAction::*;
        let mut p = plan(vec![
            step("a", "A", Pick),
            step("b", "B", Squash),
            step("c", "C", Drop),
            step("d", "D", Reword),
            step("e", "E", Fixup),
            step("f", "F", Edit),
        ]);
        p.steps[3].reword = Some("D2\n\nmore".into());
        let out = p.preview();
        assert_eq!(out.len(), 3);
        assert_eq!(out[0].summary, "A");
        assert_eq!(out[0].sources, ["aaaaaaa", "bbbbbbb"]);
        assert_eq!(out[1].summary, "D2");
        assert!(out[1].reworded);
        assert_eq!(out[1].sources.len(), 2);
        assert!(out[2].stops);
    }

    #[test]
    fn moving_and_noop_detection() {
        use RebaseAction::*;
        let mut p = plan(vec![
            step("a", "A", Pick),
            step("b", "B", Pick),
            step("c", "C", Pick),
        ]);
        let original: Vec<Oid> = p.steps.iter().map(|s| s.oid.clone()).collect();
        assert!(p.is_noop(&original));
        p.move_step(0, 9);
        assert_eq!(p.steps[2].summary(), "A");
        assert!(!p.is_noop(&original));
        p.move_step(2, 0);
        assert!(p.is_noop(&original));
        p.steps[1].action = Drop;
        assert!(!p.is_noop(&original));
        assert_eq!(RebaseAction::from_key("f"), Some(Fixup));
        assert_eq!(RebaseAction::from_key("x"), None);
    }

    #[test]
    fn summaries_never_break_todo_lines() {
        let s = step("a", "", RebaseAction::Pick);
        let p = plan(vec![s]);
        assert_eq!(p.todo(|_| String::new()).lines().count(), 1);
    }
}

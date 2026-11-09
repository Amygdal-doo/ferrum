//! Undo of the last local operation, derived from HEAD's reflog.
//!
//! Only operations that can be reversed without losing work qualify: a
//! commit (or amend) becomes a soft reset, so its changes stay staged, and a
//! checkout switches back. Discards, resets and anything touching remotes are
//! never offered.

use crate::commit::Oid;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undo {
    /// `git reset --soft <to>`: moves the branch back to before the commit.
    Commit {
        /// HEAD when the undo was planned; the undo refuses if it moved.
        head: Oid,
        to: Oid,
        summary: String,
        amend: bool,
    },
    /// Switch back to `from` (a branch name, or a commit when it was detached).
    Checkout { head: Oid, from: String, to: String },
}

impl Undo {
    /// Plans an undo from HEAD's reflog, newest first: `(new oid, subject)`
    /// pairs as printed by `git reflog --format=%H%x1f%gs`.
    #[must_use]
    pub fn from_reflog(entries: &[(Oid, String)]) -> Option<Self> {
        let (head, subject) = entries.first()?;
        if let Some(rest) = subject.strip_prefix("checkout: moving from ") {
            let (from, to) = rest.split_once(" to ")?;
            return (from != to && !from.is_empty()).then(|| Self::Checkout {
                head: head.clone(),
                from: from.to_owned(),
                to: to.to_owned(),
            });
        }
        // `commit (initial)` has nothing to go back to; merges, resets,
        // rebases, pulls … are not undone here.
        let (summary, amend) = match subject.strip_prefix("commit: ") {
            Some(s) => (s, false),
            None => (subject.strip_prefix("commit (amend): ")?, true),
        };
        let (before, _) = entries.get(1)?;
        Some(Self::Commit {
            head: head.clone(),
            to: before.clone(),
            summary: summary.to_owned(),
            amend,
        })
    }

    /// HEAD the undo was planned against.
    #[must_use]
    pub fn head(&self) -> &Oid {
        match self {
            Self::Commit { head, .. } | Self::Checkout { head, .. } => head,
        }
    }

    /// Short menu / tooltip label.
    #[must_use]
    pub fn label(&self) -> String {
        match self {
            Self::Commit {
                summary,
                amend: false,
                ..
            } => format!("Undo commit “{summary}”"),
            Self::Commit { amend: true, .. } => "Undo amend".into(),
            Self::Checkout { from, .. } => format!("Undo checkout (back to {})", short(from)),
        }
    }

    /// What will happen, for the confirmation dialog.
    #[must_use]
    pub fn description(&self) -> String {
        match self {
            Self::Commit {
                to, amend: false, ..
            } => format!(
                "The branch moves back to {}. The commit's changes stay staged, so nothing is lost.",
                to.short()
            ),
            Self::Commit {
                to, amend: true, ..
            } => format!(
                "The branch moves back to the commit before amending ({}). The amended changes stay staged.",
                to.short()
            ),
            Self::Checkout { from, to, .. } => format!(
                "Switches from {} back to {}. Uncommitted changes are carried over; git refuses if they conflict.",
                short(to),
                short(from)
            ),
        }
    }
}

/// Abbreviates a full SHA (detached checkouts), leaves branch names alone.
fn short(name: &str) -> &str {
    if name.len() == 40 && name.bytes().all(|b| b.is_ascii_hexdigit()) {
        &name[..7]
    } else {
        name
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log(entries: &[(&str, &str)]) -> Vec<(Oid, String)> {
        entries
            .iter()
            .map(|(o, s)| (Oid::new(*o), (*s).to_owned()))
            .collect()
    }

    #[test]
    fn commit_and_amend_become_soft_resets() {
        let u = Undo::from_reflog(&log(&[("b", "commit: Add x"), ("a", "commit: Base")]));
        assert_eq!(
            u,
            Some(Undo::Commit {
                head: Oid::new("b"),
                to: Oid::new("a"),
                summary: "Add x".into(),
                amend: false
            })
        );
        let u = Undo::from_reflog(&log(&[
            ("c", "commit (amend): Add x"),
            ("b", "commit: Add x"),
        ]))
        .expect("amend");
        assert!(matches!(u, Undo::Commit { amend: true, ref to, .. } if to.as_str() == "b"));
        assert_eq!(u.head().as_str(), "c");
    }

    #[test]
    fn checkout_switches_back() {
        let u = Undo::from_reflog(&log(&[("a", "checkout: moving from main to feature")]))
            .expect("checkout");
        assert_eq!(
            u,
            Undo::Checkout {
                head: Oid::new("a"),
                from: "main".into(),
                to: "feature".into()
            }
        );
        assert_eq!(u.label(), "Undo checkout (back to main)");
    }

    #[test]
    fn unsafe_or_unknown_operations_are_not_undoable() {
        for subject in [
            "commit (initial): First",
            "commit (merge): Merge branch 'x'",
            "reset: moving to HEAD~1",
            "merge feature: Fast-forward",
            "pull: Fast-forward",
            "rebase (finish): returning to refs/heads/main",
            "checkout: moving from main to main",
        ] {
            assert_eq!(
                Undo::from_reflog(&log(&[("b", subject), ("a", "commit: x")])),
                None,
                "{subject}"
            );
        }
        // A commit with nothing before it in the reflog.
        assert_eq!(Undo::from_reflog(&log(&[("b", "commit: x")])), None);
        assert_eq!(Undo::from_reflog(&[]), None);
    }
}

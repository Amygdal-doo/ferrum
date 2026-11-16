//! `git bisect`: finding the commit that introduced a bug by marking
//! commits good or bad until one is left.

use crate::commit::Oid;

/// A verdict on the commit being tested.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BisectMark {
    Good,
    Bad,
    /// Cannot be tested (does not build…).
    Skip,
}

impl BisectMark {
    /// git's subcommand.
    #[must_use]
    pub const fn command(self) -> &'static str {
        match self {
            Self::Good => "good",
            Self::Bad => "bad",
            Self::Skip => "skip",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Good => "Good",
            Self::Bad => "Bad",
            Self::Skip => "Skip",
        }
    }
}

/// A bisect session in progress (read from `.git/BISECT_*` and `refs/bisect`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct BisectState {
    /// The branch (or commit) checked out before bisecting; reset returns to it.
    pub start: String,
    pub bad: Option<Oid>,
    pub good: usize,
    pub skipped: usize,
    /// Candidates still in range (the bad commit included), once both a good
    /// and a bad commit are known.
    pub remaining: Option<usize>,
    /// The first bad commit, once found.
    pub found: Option<Oid>,
}

impl BisectState {
    /// Roughly how many more tests are needed (⌈log₂ candidates⌉).
    #[must_use]
    pub fn steps_left(&self) -> Option<u32> {
        let n = self.remaining?;
        Some(if n <= 1 { 0 } else { (n - 1).ilog2() + 1 })
    }

    /// What the banner says.
    #[must_use]
    pub fn summary(&self) -> String {
        if let Some(found) = &self.found {
            return format!("Bisect done: {} is the first bad commit", found.short());
        }
        match (self.bad.is_some(), self.good, self.remaining) {
            (false, 0, _) => "Bisecting: mark a bad and a good commit to start".into(),
            (false, _, _) => "Bisecting: mark a bad commit (e.g. the current one)".into(),
            (true, 0, _) => "Bisecting: mark a good commit (one without the bug)".into(),
            (true, _, Some(n)) => {
                let steps = self.steps_left().unwrap_or(0);
                format!(
                    "Bisecting: test the checked-out commit — {} left to test (about {steps} step{})",
                    n.saturating_sub(1),
                    if steps == 1 { "" } else { "s" }
                )
            }
            (true, _, None) => "Bisecting: test the checked-out commit".into(),
        }
    }
}

/// The first bad commit recorded in `BISECT_LOG` (`# first bad commit: [<oid>] …`).
#[must_use]
pub fn found_in_log(log: &str) -> Option<Oid> {
    log.lines().rev().find_map(|line| {
        let rest = line.strip_prefix("# first bad commit: [")?;
        let (oid, _) = rest.split_once(']')?;
        Some(Oid::new(oid))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_first_bad_commit_from_the_log() {
        let log = "git bisect start\n# bad: [aaaa] Two\ngit bisect bad aaaa\n# first bad commit: [bbbb] Broke it\n";
        assert_eq!(found_in_log(log), Some(Oid::new("bbbb")));
        assert_eq!(found_in_log("git bisect start\n"), None);
    }

    #[test]
    fn summaries_guide_the_next_step() {
        let mut s = BisectState::default();
        assert!(s.summary().contains("mark a bad and a good"));
        s.bad = Some(Oid::new("a".repeat(40)));
        assert!(s.summary().contains("mark a good"));
        s.good = 1;
        s.remaining = Some(9);
        assert_eq!(s.steps_left(), Some(4));
        assert!(s.summary().contains("8 left to test"), "{}", s.summary());
        s.remaining = Some(2);
        assert_eq!(s.steps_left(), Some(1));
        s.found = Some(Oid::new("b".repeat(40)));
        assert!(s.summary().starts_with("Bisect done: bbbbbbb"));
    }
}

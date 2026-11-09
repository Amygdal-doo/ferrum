//! Commit search over the loaded history: message, author and SHA prefix.

use crate::commit::Commit;
use std::collections::HashSet;

/// A normalised search: case-insensitive substring for message and author,
/// prefix for the SHA.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CommitQuery {
    needle: String,
}

impl CommitQuery {
    #[must_use]
    pub fn new(text: &str) -> Self {
        Self {
            needle: text.trim().to_lowercase(),
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.needle.is_empty()
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.needle
    }

    /// Summary, author name / email, or SHA prefix.
    #[must_use]
    pub fn matches(&self, c: &Commit) -> bool {
        let n = self.needle.as_str();
        !n.is_empty()
            && (c.oid.as_str().starts_with(n)
                || c.summary.to_lowercase().contains(n)
                || c.author_name.to_lowercase().contains(n)
                || c.author_email.to_lowercase().contains(n))
    }

    /// Row indices of matching commits in `commits`, in order. `extra` holds
    /// full ids matched elsewhere (e.g. message bodies found by `git log --grep`).
    #[must_use]
    pub fn find(&self, commits: &[Commit], extra: &HashSet<String>) -> Vec<usize> {
        if self.is_empty() {
            return Vec::new();
        }
        commits
            .iter()
            .enumerate()
            .filter(|(_, c)| self.matches(c) || extra.contains(c.oid.as_str()))
            .map(|(i, _)| i)
            .collect()
    }
}

/// The match after (`forward`) or before `current` (a row index, not
/// necessarily a match), wrapping around. `None` without matches.
#[must_use]
pub fn step_match(matches: &[usize], current: Option<usize>, forward: bool) -> Option<usize> {
    let (first, last) = (*matches.first()?, *matches.last()?);
    let Some(cur) = current else {
        return Some(if forward { first } else { last });
    };
    if forward {
        Some(matches.iter().copied().find(|&m| m > cur).unwrap_or(first))
    } else {
        Some(
            matches
                .iter()
                .rev()
                .copied()
                .find(|&m| m < cur)
                .unwrap_or(last),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commit::Oid;

    fn commit(oid: &str, summary: &str, author: &str) -> Commit {
        Commit {
            oid: Oid::new(oid),
            summary: summary.into(),
            author_name: author.into(),
            author_email: format!("{}@example.com", author.to_lowercase()),
            ..Default::default()
        }
    }

    #[test]
    fn matches_message_author_and_sha_prefix() {
        let c = commit("abc1234def", "Fix Parser crash", "Ada Lovelace");
        assert!(CommitQuery::new("parser").matches(&c));
        assert!(CommitQuery::new("  LOVELACE ").matches(&c));
        assert!(CommitQuery::new("lovelace@example").matches(&c));
        assert!(
            CommitQuery::new("ABC12").matches(&c),
            "SHA is case-insensitive"
        );
        assert!(
            !CommitQuery::new("1234").matches(&c),
            "SHA must be a prefix"
        );
        assert!(!CommitQuery::new("").matches(&c));
        assert!(!CommitQuery::new("   ").matches(&c));
    }

    #[test]
    fn finds_rows_including_extra_ids() {
        let log = vec![
            commit("a1", "Add search", "Ada"),
            commit("b2", "Tidy", "Bob"),
            commit("c3", "Search box", "Cy"),
        ];
        let none = HashSet::new();
        assert_eq!(CommitQuery::new("search").find(&log, &none), vec![0, 2]);
        let body = HashSet::from(["b2".to_owned()]);
        assert_eq!(CommitQuery::new("search").find(&log, &body), vec![0, 1, 2]);
        assert!(CommitQuery::new("").find(&log, &body).is_empty());
    }

    #[test]
    fn steps_through_matches_with_wraparound() {
        let m = [2, 5, 9];
        assert_eq!(step_match(&m, None, true), Some(2));
        assert_eq!(step_match(&m, None, false), Some(9));
        assert_eq!(step_match(&m, Some(2), true), Some(5));
        assert_eq!(step_match(&m, Some(9), true), Some(2));
        assert_eq!(step_match(&m, Some(2), false), Some(9));
        assert_eq!(step_match(&m, Some(5), false), Some(2));
        // From a row that is not a match.
        assert_eq!(step_match(&m, Some(6), true), Some(9));
        assert_eq!(step_match(&m, Some(6), false), Some(5));
        assert_eq!(step_match(&[], Some(1), true), None);
    }
}

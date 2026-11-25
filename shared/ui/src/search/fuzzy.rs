//! A small fuzzy matcher for file paths and command names (the same
//! matcher as FerrumIDE's `ferrumeditor_core::fuzzy`).
//!
//! The query is matched as a subsequence of the candidate. Scoring rewards
//! consecutive runs, matches at word boundaries (start, after `/`, `_`, `-`,
//! `.`, space, camelCase humps) and matches inside the basename, and lightly
//! penalises gaps. The best alignment is found with a small dynamic program so
//! `"edmod"` prefers `editor/mod.rs` over a greedy left-most alignment.
//!
//! Smart case: an all-lowercase query matches case-insensitively; a query
//! containing an uppercase letter matches case-sensitively. Spaces in the
//! query are ignored.

const SCORE_MATCH: i64 = 16;
const BONUS_CONSECUTIVE: i64 = 32;
const BONUS_START: i64 = 40;
const BONUS_AFTER_SLASH: i64 = 40;
const BONUS_BOUNDARY: i64 = 25;
const BONUS_CAMEL: i64 = 25;
const BONUS_BASENAME: i64 = 8;
const PENALTY_GAP: i64 = 3;
const PENALTY_LEADING_MAX: i64 = 10;
const NONE: i64 = i64::MIN / 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FuzzyMatch {
    pub score: i64,
    /// Char indices (not byte offsets) of the matched characters in the candidate.
    pub indices: Vec<usize>,
}

/// Matches `query` against `candidate`; `None` when it is not a subsequence.
/// An empty query matches everything with score 0.
pub fn fuzzy_match(query: &str, candidate: &str) -> Option<FuzzyMatch> {
    let query: Vec<char> = query.chars().filter(|c| !c.is_whitespace()).collect();
    if query.is_empty() {
        return Some(FuzzyMatch {
            score: 0,
            indices: Vec::new(),
        });
    }
    let case_sensitive = query.iter().any(|c| c.is_uppercase());
    let chars: Vec<char> = candidate.chars().collect();
    let folded: Vec<char> = if case_sensitive {
        chars.clone()
    } else {
        chars.iter().map(|c| fold(*c)).collect()
    };
    let query: Vec<char> = if case_sensitive {
        query
    } else {
        query.into_iter().map(fold).collect()
    };
    if !is_subsequence(&query, &folded) {
        return None;
    }

    let n = chars.len();
    let m = query.len();
    let basename_start = chars
        .iter()
        .rposition(|c| *c == '/' || *c == '\\')
        .map_or(0, |i| i + 1);
    let char_bonus: Vec<i64> = (0..n).map(|j| boundary_bonus(&chars, j)).collect();

    // score[i][j]: best score with query[i] matched at candidate j.
    let mut score = vec![vec![NONE; n]; m];
    let mut prev = vec![vec![usize::MAX; n]; m];
    for i in 0..m {
        // Best predecessor with a gap: max over k < j - 1 of score[i-1][k] + k * gap.
        let mut best_gap = NONE;
        let mut best_gap_index = usize::MAX;
        for j in 0..n {
            if i > 0 && j >= 2 {
                let k = j - 2;
                let candidate_value = score[i - 1][k].saturating_add(k as i64 * PENALTY_GAP);
                if score[i - 1][k] > NONE && candidate_value > best_gap {
                    best_gap = candidate_value;
                    best_gap_index = k;
                }
            }
            if folded[j] != query[i] {
                continue;
            }
            let mut base = SCORE_MATCH + char_bonus[j];
            if j >= basename_start {
                base += BONUS_BASENAME;
            }
            if i == 0 {
                score[i][j] = base - (j as i64).min(PENALTY_LEADING_MAX);
                continue;
            }
            let mut best = NONE;
            let mut best_prev = usize::MAX;
            if j >= 1 && score[i - 1][j - 1] > NONE {
                best = score[i - 1][j - 1] + BONUS_CONSECUTIVE;
                best_prev = j - 1;
            }
            if best_gap > NONE {
                let gapped = best_gap - (j as i64 - 1) * PENALTY_GAP;
                if gapped > best {
                    best = gapped;
                    best_prev = best_gap_index;
                }
            }
            if best > NONE {
                score[i][j] = best + base;
                prev[i][j] = best_prev;
            }
        }
    }

    let (mut j, best) = score[m - 1]
        .iter()
        .enumerate()
        .filter(|(_, s)| **s > NONE)
        .max_by_key(|(_, s)| **s)
        .map(|(j, s)| (j, *s))?;
    let mut indices = vec![0; m];
    for i in (0..m).rev() {
        indices[i] = j;
        if i > 0 {
            j = prev[i][j];
            if j == usize::MAX {
                return None;
            }
        }
    }
    Some(FuzzyMatch {
        score: best,
        indices,
    })
}

/// Filters and ranks `items` by `key`, best first (ties: shorter key, then
/// original order). Returns `(item index, match)`, at most `limit` entries.
/// An empty query keeps every item in its original order.
pub fn fuzzy_filter<T>(
    query: &str,
    items: &[T],
    key: impl Fn(&T) -> &str,
    limit: usize,
) -> Vec<(usize, FuzzyMatch)> {
    let mut results: Vec<(usize, FuzzyMatch, usize)> = items
        .iter()
        .enumerate()
        .filter_map(|(index, item)| {
            let text = key(item);
            fuzzy_match(query, text).map(|m| (index, m, text.chars().count()))
        })
        .collect();
    results.sort_by(|a, b| {
        b.1.score
            .cmp(&a.1.score)
            .then(a.2.cmp(&b.2))
            .then(a.0.cmp(&b.0))
    });
    results.truncate(limit);
    results
        .into_iter()
        .map(|(index, m, _)| (index, m))
        .collect()
}

fn fold(c: char) -> char {
    c.to_lowercase().next().unwrap_or(c)
}

fn is_subsequence(query: &[char], candidate: &[char]) -> bool {
    let mut q = query.iter().peekable();
    for c in candidate {
        if q.peek() == Some(&c) {
            q.next();
        }
    }
    q.peek().is_none()
}

fn boundary_bonus(chars: &[char], j: usize) -> i64 {
    if j == 0 {
        return BONUS_START;
    }
    let previous = chars[j - 1];
    let current = chars[j];
    match previous {
        '/' | '\\' => BONUS_AFTER_SLASH,
        '_' | '-' | '.' | ' ' | ':' => BONUS_BOUNDARY,
        _ if previous.is_lowercase() && current.is_uppercase() => BONUS_CAMEL,
        _ if !previous.is_alphanumeric() && current.is_alphanumeric() => BONUS_BOUNDARY,
        _ => 0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_subsequences_and_prefers_word_starts() {
        assert!(fuzzy_match("xyz", "editor").is_none());
        let ranked = fuzzy_filter("edmod", &["src/feed_model.rs", "editor/mod.rs"], |s| s, 10);
        assert_eq!(ranked[0].0, 1);
    }

    #[test]
    fn empty_query_keeps_order() {
        let ranked = fuzzy_filter("", &["b", "a"], |s| s, 10);
        assert_eq!(ranked.iter().map(|r| r.0).collect::<Vec<_>>(), vec![0, 1]);
    }
}

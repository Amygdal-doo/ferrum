//! A small fuzzy matcher for file paths and command names.
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

    fn ranked<'a>(query: &str, items: &[&'a str]) -> Vec<&'a str> {
        fuzzy_filter(query, items, |s| s, 100)
            .into_iter()
            .map(|(i, _)| items[i])
            .collect()
    }

    #[test]
    fn subsequence_required() {
        assert!(fuzzy_match("abc", "a_b_c").is_some());
        assert!(fuzzy_match("abd", "abc").is_none());
        assert!(fuzzy_match("cba", "abc").is_none());
    }

    #[test]
    fn empty_query_matches_everything_in_order() {
        assert_eq!(ranked("", &["b", "a", "c"]), vec!["b", "a", "c"]);
        assert_eq!(ranked("  ", &["x"]), vec!["x"]);
    }

    #[test]
    fn indices_are_char_indices() {
        let m = fuzzy_match("ab", "äxab").expect("matches");
        assert_eq!(m.indices, vec![2, 3]);
    }

    #[test]
    fn smart_case() {
        assert!(fuzzy_match("foo", "FooBar").is_some());
        assert!(fuzzy_match("Foo", "foobar").is_none());
        assert!(fuzzy_match("FB", "FooBar").is_some());
    }

    #[test]
    fn spaces_in_query_are_ignored() {
        assert!(fuzzy_match("ed mod", "editor/mod.rs").is_some());
    }

    #[test]
    fn edmod_prefers_editor_mod() {
        let items = ["end_mode_old.rs", "editor/mod.rs"];
        assert_eq!(ranked("edmod", &items)[0], "editor/mod.rs");
    }

    #[test]
    fn basename_preferred() {
        let items = ["main_dir/other.rs", "src/main.rs"];
        assert_eq!(ranked("main", &items)[0], "src/main.rs");
    }

    #[test]
    fn consecutive_beats_scattered() {
        let items = ["a/b/u/f/f/e/r.rs", "src/buffer.rs"];
        assert_eq!(ranked("buffer", &items)[0], "src/buffer.rs");
    }

    #[test]
    fn word_boundary_beats_mid_word() {
        let items = ["xfoobar.rs", "foo_bar.rs"];
        assert_eq!(ranked("fb", &items)[0], "foo_bar.rs");
    }

    #[test]
    fn camel_case_humps() {
        let items = ["filebrowser.ts", "FileBrowser.ts"];
        assert_eq!(ranked("FB", &items), vec!["FileBrowser.ts"]);
        let m = fuzzy_match("fb", "FileBrowser.ts").expect("matches");
        assert_eq!(m.indices, vec![0, 4]);
    }

    #[test]
    fn shorter_wins_ties() {
        let items = ["readme.markdown", "readme.md"];
        assert_eq!(ranked("readme", &items)[0], "readme.md");
    }

    #[test]
    fn best_alignment_found_not_greedy() {
        // Greedy would take the first `m`; the best alignment uses the basename.
        let m = fuzzy_match("mod", "my/dir/mod.rs").expect("matches");
        assert_eq!(m.indices, vec![7, 8, 9]);
    }

    #[test]
    fn command_names() {
        let items = [
            "Editor: Duplicate Line",
            "Editor: Delete Line",
            "Tree View: Toggle",
        ];
        assert_eq!(ranked("dupl", &items), vec!["Editor: Duplicate Line"]);
        assert_eq!(ranked("tvt", &items)[0], "Tree View: Toggle");
    }

    #[test]
    fn limit_is_applied() {
        let items = ["a1", "a2", "a3"];
        assert_eq!(fuzzy_filter("a", &items, |s| s, 2).len(), 2);
    }
}

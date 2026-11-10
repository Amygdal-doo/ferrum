//! Regex tester on the Rust `regex` crate (its syntax: no look-around or
//! backreferences).

use regex::RegexBuilder;
use std::ops::Range;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Flags {
    /// `i`
    pub case_insensitive: bool,
    /// `m`: `^`/`$` match at line boundaries.
    pub multi_line: bool,
    /// `s`: `.` matches `\n`.
    pub dot_all: bool,
    /// `x`: ignore whitespace and allow `#` comments.
    pub extended: bool,
}

impl Flags {
    /// Parses a flag string such as `"im"`. Unknown letters are an error.
    pub fn parse(letters: &str) -> Result<Self, String> {
        let mut flags = Flags::default();
        for letter in letters.chars().filter(|c| !c.is_whitespace()) {
            match letter {
                'i' => flags.case_insensitive = true,
                'm' => flags.multi_line = true,
                's' => flags.dot_all = true,
                'x' => flags.extended = true,
                // `g` is implied: all matches are always listed.
                'g' => {}
                other => return Err(format!("Unknown flag '{other}' (use i, m, s, x)")),
            }
        }
        Ok(flags)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group {
    /// 1-based group index.
    pub index: usize,
    pub name: Option<String>,
    /// `None` if the group did not participate in the match.
    pub range: Option<Range<usize>>,
    pub text: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Match {
    /// Byte range in the test text.
    pub range: Range<usize>,
    pub text: String,
    pub groups: Vec<Group>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RegexResult {
    pub matches: Vec<Match>,
    /// True if the match list stopped at [`MAX_MATCHES`].
    pub truncated: bool,
}

pub const MAX_MATCHES: usize = 1_000;

/// Finds all non-overlapping matches of `pattern` in `text`.
pub fn find_matches(pattern: &str, flags: Flags, text: &str) -> Result<RegexResult, String> {
    if pattern.is_empty() {
        return Ok(RegexResult {
            matches: Vec::new(),
            truncated: false,
        });
    }
    let regex = RegexBuilder::new(pattern)
        .case_insensitive(flags.case_insensitive)
        .multi_line(flags.multi_line)
        .dot_matches_new_line(flags.dot_all)
        .ignore_whitespace(flags.extended)
        .size_limit(10 * (1 << 20))
        .build()
        .map_err(|e| e.to_string())?;
    let names: Vec<Option<String>> = regex
        .capture_names()
        .map(|name| name.map(str::to_string))
        .collect();
    let mut matches = Vec::new();
    let mut truncated = false;
    for captures in regex.captures_iter(text) {
        if matches.len() == MAX_MATCHES {
            truncated = true;
            break;
        }
        let Some(whole) = captures.get(0) else {
            continue;
        };
        let groups = (1..captures.len())
            .map(|index| {
                let group = captures.get(index);
                Group {
                    index,
                    name: names.get(index).cloned().flatten(),
                    range: group.map(|g| g.range()),
                    text: group.map(|g| g.as_str().to_string()),
                }
            })
            .collect();
        matches.push(Match {
            range: whole.range(),
            text: whole.as_str().to_string(),
            groups,
        });
    }
    Ok(RegexResult { matches, truncated })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_all_matches_with_groups() {
        let result = find_matches(
            r"(\w+)@(?<host>\w+)\.com",
            Flags::default(),
            "a@x.com, bob@yy.com",
        )
        .unwrap();
        assert_eq!(result.matches.len(), 2);
        let second = &result.matches[1];
        assert_eq!(second.text, "bob@yy.com");
        assert_eq!(second.range, 9..19);
        assert_eq!(second.groups[0].text.as_deref(), Some("bob"));
        assert_eq!(second.groups[1].name.as_deref(), Some("host"));
        assert_eq!(second.groups[1].text.as_deref(), Some("yy"));
        assert!(!result.truncated);
    }

    #[test]
    fn optional_group_that_did_not_match() {
        let result = find_matches(r"a(b)?", Flags::default(), "a").unwrap();
        assert_eq!(result.matches[0].groups[0].range, None);
    }

    #[test]
    fn flags_change_matching() {
        let text = "Foo\nfoo";
        assert_eq!(
            find_matches("foo", Flags::default(), text)
                .unwrap()
                .matches
                .len(),
            1
        );
        let i = Flags::parse("i").unwrap();
        assert_eq!(find_matches("foo", i, text).unwrap().matches.len(), 2);
        let m = Flags::parse("mi").unwrap();
        assert_eq!(find_matches("^foo$", m, text).unwrap().matches.len(), 2);
        let s = Flags::parse("s").unwrap();
        assert_eq!(
            find_matches("o.f", s, text).unwrap().matches[0].text,
            "o\nf"
        );
        let x = Flags::parse("x").unwrap();
        assert_eq!(
            find_matches("f o o # comment", x, text)
                .unwrap()
                .matches
                .len(),
            1
        );
    }

    #[test]
    fn parses_flags() {
        assert_eq!(Flags::parse("").unwrap(), Flags::default());
        assert!(Flags::parse("g").is_ok());
        assert!(Flags::parse("q").unwrap_err().contains("'q'"));
    }

    #[test]
    fn unicode_byte_ranges() {
        let result = find_matches("é+", Flags::default(), "caféé!").unwrap();
        assert_eq!(result.matches[0].range, 3..7);
    }

    #[test]
    fn invalid_and_unsupported_patterns_error() {
        assert!(find_matches("(", Flags::default(), "x").is_err());
        // Look-around is not supported by the Rust regex crate.
        assert!(find_matches("(?=a)", Flags::default(), "a").is_err());
    }

    #[test]
    fn empty_pattern_has_no_matches_and_cap_applies() {
        assert!(
            find_matches("", Flags::default(), "abc")
                .unwrap()
                .matches
                .is_empty()
        );
        let text = "a".repeat(MAX_MATCHES + 10);
        let result = find_matches("a", Flags::default(), &text).unwrap();
        assert_eq!(result.matches.len(), MAX_MATCHES);
        assert!(result.truncated);
    }
}

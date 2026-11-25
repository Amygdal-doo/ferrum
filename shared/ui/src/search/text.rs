//! Matching a [`TextQuery`] against text the app already holds (request
//! bodies, logs, saved queries), for apps whose [`TextSearcher`] is not a
//! walk over files.

use anyhow::Context as _;
use globset::{Glob, GlobSet, GlobSetBuilder};
use regex::{Regex, RegexBuilder};

use super::{TextHit, TextQuery};

/// Longest preview kept around a match, in bytes.
const PREVIEW_MAX: usize = 240;

/// A compiled [`TextQuery`].
pub struct TextMatcher {
    regex: Regex,
    include: Option<GlobSet>,
    exclude: Option<GlobSet>,
}

impl TextMatcher {
    /// Fails on an empty query, a bad regex or a bad mask.
    pub fn new(query: &TextQuery) -> anyhow::Result<Self> {
        if query.text.is_empty() {
            anyhow::bail!("type something to search for");
        }
        let mut pattern = if query.regex {
            query.text.clone()
        } else {
            regex::escape(&query.text)
        };
        if query.whole_word {
            pattern = format!(r"\b(?:{pattern})\b");
        }
        let regex = RegexBuilder::new(&pattern)
            .case_insensitive(!query.case_sensitive)
            .build()
            .context("not a valid regular expression")?;
        let (mut include, mut exclude) = (GlobSetBuilder::new(), GlobSetBuilder::new());
        let (mut any_include, mut any_exclude) = (false, false);
        for part in query
            .mask
            .split(',')
            .map(str::trim)
            .filter(|p| !p.is_empty())
        {
            if let Some(glob) = part.strip_prefix('!') {
                exclude.add(Glob::new(glob.trim()).with_context(|| format!("bad mask “{part}”"))?);
                any_exclude = true;
            } else {
                include.add(Glob::new(part).with_context(|| format!("bad mask “{part}”"))?);
                any_include = true;
            }
        }
        Ok(Self {
            regex,
            include: any_include.then(|| include.build()).transpose()?,
            exclude: any_exclude.then(|| exclude.build()).transpose()?,
        })
    }

    /// Whether the mask lets `name` (a path or a label) through; a glob
    /// matches the whole name or its last `/` component.
    pub fn allows(&self, name: &str) -> bool {
        let base = name.rsplit('/').next().unwrap_or(name);
        let hit = |set: &GlobSet| set.is_match(name) || set.is_match(base);
        self.include.as_ref().is_none_or(hit) && !self.exclude.as_ref().is_some_and(hit)
    }

    /// Appends every matching line of `text` to `out`, up to `limit` hits in
    /// total. Returns false once the limit is reached.
    pub fn find_lines(
        &self,
        text: &str,
        source: &str,
        source_label: &str,
        limit: usize,
        out: &mut Vec<TextHit>,
    ) -> bool {
        for (line, content) in text.lines().enumerate() {
            let Some(m) = self.regex.find(content) else {
                continue;
            };
            if out.len() >= limit {
                return false;
            }
            let (preview, range) = preview(content, m.range());
            out.push(TextHit {
                source: source.to_owned().into(),
                source_label: source_label.to_owned().into(),
                line,
                column: m.start(),
                preview: preview.into(),
                range,
            });
        }
        out.len() < limit
    }
}

/// The line, cut to [`PREVIEW_MAX`] bytes around the match, with the match
/// range moved into the cut.
fn preview(line: &str, range: std::ops::Range<usize>) -> (String, std::ops::Range<usize>) {
    let line_trimmed = line.trim_end();
    let lead = line_trimmed.len() - line_trimmed.trim_start().len();
    let mut start = lead.min(range.start);
    if line_trimmed.len() - start > PREVIEW_MAX && range.start > start + 40 {
        start = range.start - 40;
    }
    while !line.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + PREVIEW_MAX)
        .min(line_trimmed.len())
        .max(range.end.min(line.len()));
    while !line.is_char_boundary(end) {
        end += 1;
    }
    let cut = &line[start..end];
    let prefix = if start > lead { "…" } else { "" };
    let shift = prefix.len() as isize - start as isize;
    let moved = (range.start as isize + shift) as usize..(range.end as isize + shift) as usize;
    (format!("{prefix}{cut}"), moved)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn q(text: &str) -> TextQuery {
        TextQuery {
            text: text.into(),
            ..TextQuery::default()
        }
    }

    #[test]
    fn finds_lines_with_options() {
        let text = "let Foo = 1;\n  foo();\nfoobar\n";
        let mut out = Vec::new();
        TextMatcher::new(&q("foo"))
            .expect("matcher")
            .find_lines(text, "a", "a", 10, &mut out);
        assert_eq!(
            out.iter().map(|h| h.line).collect::<Vec<_>>(),
            vec![0, 1, 2]
        );
        assert_eq!(&out[1].preview[out[1].range.clone()], "foo");

        let mut out = Vec::new();
        let query = TextQuery {
            case_sensitive: true,
            whole_word: true,
            ..q("foo")
        };
        TextMatcher::new(&query)
            .expect("matcher")
            .find_lines(text, "a", "a", 10, &mut out);
        assert_eq!(out.iter().map(|h| h.line).collect::<Vec<_>>(), vec![1]);

        let query = TextQuery {
            regex: true,
            ..q("f(o+)bar")
        };
        let mut out = Vec::new();
        TextMatcher::new(&query)
            .expect("matcher")
            .find_lines(text, "a", "a", 10, &mut out);
        assert_eq!(out.len(), 1);
        assert!(
            TextMatcher::new(&TextQuery {
                regex: true,
                ..q("(")
            })
            .is_err()
        );
    }

    #[test]
    fn stops_at_the_limit() {
        let mut out = Vec::new();
        let more = TextMatcher::new(&q("a")).expect("matcher").find_lines(
            "a\na\na\n",
            "s",
            "s",
            2,
            &mut out,
        );
        assert!(!more);
        assert_eq!(out.len(), 2);
    }

    #[test]
    fn masks_include_and_exclude() {
        let m = TextMatcher::new(&TextQuery {
            mask: "*.rs, !*_test.rs".into(),
            ..q("x")
        })
        .expect("matcher");
        assert!(m.allows("src/main.rs"));
        assert!(!m.allows("src/main_test.rs"));
        assert!(!m.allows("README.md"));
        assert!(
            TextMatcher::new(&q("x"))
                .expect("matcher")
                .allows("anything")
        );
    }

    #[test]
    fn long_lines_are_cut_around_the_match() {
        let line = format!("{}needle{}", "x".repeat(500), "y".repeat(500));
        let mut out = Vec::new();
        TextMatcher::new(&q("needle"))
            .expect("matcher")
            .find_lines(&line, "s", "s", 5, &mut out);
        let hit = &out[0];
        assert!(hit.preview.len() <= PREVIEW_MAX + 10);
        assert_eq!(&hit.preview[hit.range.clone()], "needle");
    }
}

//! Find in files: walks a folder (respecting `.gitignore`), skips binary and
//! oversized files, and reports matches per line.

use std::ops::Range;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::find::{FindError, SearchQuery};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchLimits {
    /// Stop after this many matches (the result is marked `truncated`).
    pub max_results: usize,
    /// Files larger than this (bytes) are skipped.
    pub max_file_size: u64,
    /// Lines longer than this (bytes) are shown as a window around the match.
    pub max_line_len: usize,
}

impl Default for SearchLimits {
    fn default() -> Self {
        Self {
            max_results: 10_000,
            max_file_size: 4 * 1024 * 1024,
            max_line_len: 400,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LineMatch {
    /// 0-based line number.
    pub line: usize,
    /// Byte range of the match within `line_text` (clamped to the line).
    pub byte_range: Range<usize>,
    /// The line (or, for very long lines, a window of it around the match).
    pub line_text: String,
    /// Byte offset of `line_text`'s first byte within the full line; 0 unless
    /// the line was windowed. `byte_range.start + line_offset` is the column
    /// in the real line.
    pub line_offset: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileMatch {
    /// Absolute path (root joined with the walked path).
    pub path: PathBuf,
    pub matches: Vec<LineMatch>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ProjectSearchResult {
    pub files: Vec<FileMatch>,
    pub total_matches: usize,
    pub files_searched: usize,
    pub truncated: bool,
    pub cancelled: bool,
}

/// Searches every text file under `root`. Setting `cancel` stops the walk at
/// the next file; the partial result is returned with `cancelled = true`.
pub fn search_project(
    root: &Path,
    query: &SearchQuery,
    limits: &SearchLimits,
    cancel: &AtomicBool,
) -> Result<ProjectSearchResult, FindError> {
    let compiled = query.compile()?;
    let mut result = ProjectSearchResult::default();
    for entry in walker(root).build() {
        if cancel.load(Ordering::Relaxed) {
            result.cancelled = true;
            break;
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_some_and(|t| t.is_file()) {
            continue;
        }
        let path = entry.path();
        if entry
            .metadata()
            .map_or(true, |m| m.len() > limits.max_file_size)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(path) else {
            continue;
        };
        if bytes.iter().take(8192).any(|b| *b == 0) {
            continue;
        }
        let Ok(text) = String::from_utf8(bytes) else {
            continue;
        };
        result.files_searched += 1;

        let ranges = compiled.find_all(&text);
        if ranges.is_empty() {
            continue;
        }
        let mut matches = Vec::new();
        let mut line = 0;
        let mut line_start = 0;
        let mut scanned = 0;
        for range in ranges {
            if result.total_matches >= limits.max_results {
                result.truncated = true;
                break;
            }
            for (i, b) in text.as_bytes()[scanned..range.start].iter().enumerate() {
                if *b == b'\n' {
                    line += 1;
                    line_start = scanned + i + 1;
                }
            }
            scanned = range.start;
            let line_end = text[line_start..]
                .find('\n')
                .map_or(text.len(), |i| line_start + i);
            let full = text[line_start..line_end].trim_end_matches('\r');
            let start = range.start - line_start;
            let end = (range.end.min(line_end) - line_start)
                .min(full.len())
                .max(start);
            matches.push(windowed(full, line, start..end, limits.max_line_len));
            result.total_matches += 1;
        }
        if !matches.is_empty() {
            result.files.push(FileMatch {
                path: path.to_path_buf(),
                matches,
            });
        }
        if result.truncated {
            break;
        }
    }
    Ok(result)
}

/// A walker that honours `.gitignore` (even outside a git repo), shows
/// dotfiles, never enters `.git`, and visits entries in name order.
pub(crate) fn walker(root: &Path) -> ignore::WalkBuilder {
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        .git_ignore(true)
        .git_global(true)
        .git_exclude(true)
        .require_git(false)
        .parents(true)
        .sort_by_file_name(|a, b| a.cmp(b))
        .filter_entry(|entry| entry.file_name() != ".git");
    builder
}

fn windowed(full: &str, line: usize, range: Range<usize>, max_len: usize) -> LineMatch {
    if full.len() <= max_len {
        return LineMatch {
            line,
            byte_range: range,
            line_text: full.to_string(),
            line_offset: 0,
        };
    }
    let mut start = range.start.saturating_sub(max_len / 4);
    while !full.is_char_boundary(start) {
        start -= 1;
    }
    let mut end = (start + max_len)
        .min(full.len())
        .max(range.end.min(full.len()));
    while !full.is_char_boundary(end) {
        end += 1;
    }
    LineMatch {
        line,
        byte_range: range.start - start..range.end.min(end) - start,
        line_text: full[start..end].to_string(),
        line_offset: start,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn project() -> tempfile::TempDir {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        fs::create_dir_all(root.join("src")).expect("mkdir");
        fs::create_dir_all(root.join("target")).expect("mkdir");
        fs::create_dir_all(root.join(".git")).expect("mkdir");
        fs::write(root.join(".gitignore"), "target/\n").expect("write");
        fs::write(
            root.join("src/a.rs"),
            "fn main() {\n    let needle = 1;\n}\n",
        )
        .expect("write");
        fs::write(root.join("src/b.rs"), "needle needle\nnothing\nNEEDLE\n").expect("write");
        fs::write(root.join("target/out.rs"), "needle\n").expect("write");
        fs::write(root.join(".git/config"), "needle\n").expect("write");
        fs::write(root.join("bin.dat"), b"needle\0\x01").expect("write");
        dir
    }

    fn run(root: &Path, query: SearchQuery) -> ProjectSearchResult {
        search_project(
            root,
            &query,
            &SearchLimits::default(),
            &AtomicBool::new(false),
        )
        .expect("search")
    }

    #[test]
    fn finds_matches_and_respects_gitignore() {
        let dir = project();
        let result = run(dir.path(), SearchQuery::plain("needle"));
        let names: Vec<_> = result
            .files
            .iter()
            .map(|f| {
                f.path
                    .strip_prefix(dir.path())
                    .expect("prefix")
                    .to_path_buf()
            })
            .collect();
        assert_eq!(
            names,
            vec![PathBuf::from("src/a.rs"), PathBuf::from("src/b.rs")]
        );
        assert_eq!(result.total_matches, 4);
        assert!(!result.truncated && !result.cancelled);
        let a = &result.files[0].matches[0];
        assert_eq!(a.line, 1);
        assert_eq!(a.byte_range, 8..14);
        assert_eq!(a.line_text, "    let needle = 1;");
        let b = &result.files[1].matches;
        assert_eq!(b.iter().map(|m| m.line).collect::<Vec<_>>(), vec![0, 0, 2]);
        assert_eq!(b[1].byte_range, 7..13);
    }

    #[test]
    fn case_sensitive_and_regex() {
        let dir = project();
        let result = run(
            dir.path(),
            SearchQuery::plain("NEEDLE").case_sensitive(true),
        );
        assert_eq!(result.total_matches, 1);
        let result = run(dir.path(), SearchQuery::plain(r"let \w+").regex(true));
        assert_eq!(result.total_matches, 1);
        assert_eq!(result.files[0].matches[0].byte_range, 4..14);
    }

    #[test]
    fn invalid_query_is_an_error() {
        let dir = project();
        let error = search_project(
            dir.path(),
            &SearchQuery::plain("(").regex(true),
            &SearchLimits::default(),
            &AtomicBool::new(false),
        );
        assert!(matches!(error, Err(FindError::InvalidRegex(_))));
    }

    #[test]
    fn max_results_truncates() {
        let dir = project();
        let limits = SearchLimits {
            max_results: 2,
            ..SearchLimits::default()
        };
        let result = search_project(
            dir.path(),
            &SearchQuery::plain("needle"),
            &limits,
            &AtomicBool::new(false),
        )
        .expect("search");
        assert_eq!(result.total_matches, 2);
        assert!(result.truncated);
    }

    #[test]
    fn cancel_stops_the_walk() {
        let dir = project();
        let result = search_project(
            dir.path(),
            &SearchQuery::plain("needle"),
            &SearchLimits::default(),
            &AtomicBool::new(true),
        )
        .expect("search");
        assert!(result.cancelled);
        assert!(result.files.is_empty());
    }

    #[test]
    fn oversized_files_are_skipped() {
        let dir = project();
        let limits = SearchLimits {
            max_file_size: 10,
            ..SearchLimits::default()
        };
        let result = search_project(
            dir.path(),
            &SearchQuery::plain("needle"),
            &limits,
            &AtomicBool::new(false),
        )
        .expect("search");
        assert!(result.files.is_empty());
    }

    #[test]
    fn long_lines_are_windowed() {
        let line = format!("{}needle{}", "x".repeat(1000), "y".repeat(1000));
        let m = windowed(&line, 3, 1000..1006, 400);
        assert!(m.line_text.len() <= 400);
        assert_eq!(&m.line_text[m.byte_range.clone()], "needle");
        assert_eq!(m.byte_range.start + m.line_offset, 1000);
    }
}

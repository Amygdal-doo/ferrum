//! Following files by polling: new bytes are read from the last offset;
//! a file that shrank (truncated) or was replaced (rotated) is read again
//! from the start. Globs are re-resolved now and then to pick up new files.

use crate::buffer::Splitter;
use crate::glob;
use std::collections::BTreeMap;
use std::fs::File;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// How much of an existing file is shown when following starts.
pub const INITIAL_BYTES: u64 = 256 * 1024;
/// Upper bound read per file per poll, so a huge burst can't stall.
const MAX_READ: u64 = 4 * 1024 * 1024;
/// How often a glob is resolved again.
const RESOLVE_EVERY: Duration = Duration::from_secs(2);

/// What a poll found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TailEvent {
    /// Complete lines of `file`.
    Lines { file: PathBuf, lines: Vec<String> },
    /// Something worth telling the user (`… was truncated`).
    Note(String),
}

#[derive(Debug)]
struct Followed {
    offset: u64,
    identity: Option<(u64, u64)>,
    splitter: Splitter,
    /// Seen at least once (missing files are reported once).
    present: bool,
}

#[derive(Debug)]
pub struct FileTail {
    project: PathBuf,
    pattern: String,
    home: Option<PathBuf>,
    files: BTreeMap<PathBuf, Followed>,
    resolved_at: Option<Instant>,
    started: bool,
}

impl FileTail {
    #[must_use]
    pub fn new(project: &Path, pattern: &str, home: Option<PathBuf>) -> Self {
        Self {
            project: project.to_path_buf(),
            pattern: pattern.trim().to_owned(),
            home,
            files: BTreeMap::new(),
            resolved_at: None,
            started: false,
        }
    }

    /// The files being followed.
    pub fn files(&self) -> impl Iterator<Item = &Path> {
        self.files.keys().map(PathBuf::as_path)
    }

    /// Reads whatever is new. The first call shows the end of existing files.
    pub fn poll(&mut self) -> Vec<TailEvent> {
        let mut events = Vec::new();
        let first = !self.started;
        self.started = true;
        if self
            .resolved_at
            .is_none_or(|at| at.elapsed() >= RESOLVE_EVERY)
        {
            self.resolved_at = Some(Instant::now());
            for path in glob::resolve(&self.project, &self.pattern, self.home.as_deref()) {
                self.files.entry(path).or_insert_with(|| Followed {
                    offset: 0,
                    identity: None,
                    splitter: Splitter::default(),
                    present: false,
                });
            }
            if first && self.files.is_empty() {
                events.push(TailEvent::Note(format!(
                    "No files match {} yet; waiting for them",
                    self.pattern
                )));
            }
        }
        for (path, followed) in &mut self.files {
            read_new(path, followed, first, &mut events);
        }
        events
    }
}

fn read_new(path: &Path, followed: &mut Followed, first: bool, events: &mut Vec<TailEvent>) {
    let Ok(meta) = std::fs::metadata(path) else {
        if first {
            events.push(TailEvent::Note(format!(
                "{} does not exist yet; waiting for it",
                path.display()
            )));
        }
        followed.present = false;
        return;
    };
    let len = meta.len();
    let identity = identity(&meta);
    let mut skip_partial = false;
    if !followed.present {
        followed.present = true;
        followed.identity = identity;
        // Existing files at start: only their end. Files that appear
        // later are new: read them whole.
        if first && len > INITIAL_BYTES {
            followed.offset = len - INITIAL_BYTES;
            skip_partial = true;
        } else {
            followed.offset = 0;
            followed.splitter.reset();
        }
    } else if identity != followed.identity {
        events.push(TailEvent::Note(format!(
            "{} was replaced (rotated); reading the new file",
            path.display()
        )));
        followed.identity = identity;
        followed.offset = 0;
        followed.splitter.reset();
    } else if len < followed.offset {
        events.push(TailEvent::Note(format!("{} was truncated", path.display())));
        followed.offset = 0;
        followed.splitter.reset();
    }
    if len <= followed.offset {
        return;
    }
    let Ok(mut file) = File::open(path) else {
        return;
    };
    if file.seek(SeekFrom::Start(followed.offset)).is_err() {
        return;
    }
    let want = (len - followed.offset).min(MAX_READ);
    let mut bytes = Vec::new();
    let Ok(read) = file.take(want).read_to_end(&mut bytes) else {
        return;
    };
    followed.offset += read as u64;
    let mut slice = bytes.as_slice();
    if skip_partial {
        // Started mid-file: drop the cut-off first line.
        let start = slice.iter().position(|&b| b == b'\n').map_or(0, |i| i + 1);
        slice = &slice[start..];
    }
    let mut lines = Vec::new();
    followed
        .splitter
        .feed(slice, |line| lines.push(line.to_owned()));
    if !lines.is_empty() {
        events.push(TailEvent::Lines {
            file: path.to_path_buf(),
            lines,
        });
    }
}

#[cfg(unix)]
fn identity(meta: &std::fs::Metadata) -> Option<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    Some((meta.dev(), meta.ino()))
}

#[cfg(not(unix))]
fn identity(_: &std::fs::Metadata) -> Option<(u64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn lines(events: &[TailEvent]) -> Vec<String> {
        events
            .iter()
            .filter_map(|e| match e {
                TailEvent::Lines { lines, .. } => Some(lines.clone()),
                TailEvent::Note(_) => None,
            })
            .flatten()
            .collect()
    }

    fn append(path: &Path, text: &str) {
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .expect("open");
        file.write_all(text.as_bytes()).expect("write");
    }

    #[test]
    fn follows_appends_truncation_and_rotation() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("app.log");
        append(&path, "old 1\nold 2\n");
        let mut tail = FileTail::new(dir.path(), "app.log", None);
        assert_eq!(lines(&tail.poll()), ["old 1", "old 2"]);
        assert!(tail.poll().is_empty());
        append(&path, "new\npart");
        assert_eq!(lines(&tail.poll()), ["new"]);
        append(&path, "ial\n");
        assert_eq!(lines(&tail.poll()), ["partial"]);

        // Truncated in place.
        std::fs::write(&path, "fresh\n").expect("truncate");
        let events = tail.poll();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TailEvent::Note(n) if n.contains("truncated")))
        );
        assert_eq!(lines(&events), ["fresh"]);

        // Rotated: renamed away, a new file in its place.
        std::fs::rename(&path, dir.path().join("app.log.1")).expect("rename");
        append(&path, "rotated\n");
        let events = tail.poll();
        assert!(
            events
                .iter()
                .any(|e| matches!(e, TailEvent::Note(n) if n.contains("rotated")))
        );
        assert_eq!(lines(&events), ["rotated"]);
        assert_eq!(tail.files().count(), 1);
    }

    #[test]
    fn starts_near_the_end_of_big_files_and_waits_for_missing_ones() {
        let dir = tempfile::tempdir().expect("tempdir");
        let big = dir.path().join("big.log");
        let line = "x".repeat(99) + "\n";
        append(&big, &line.repeat(5_000));
        let mut tail = FileTail::new(dir.path(), "big.log", None);
        let got = lines(&tail.poll());
        assert!(got.len() < 3_000 && got.len() > 2_000, "{}", got.len());
        assert!(got.iter().all(|l| l.len() == 99), "no cut-off first line");

        let mut missing = FileTail::new(dir.path(), "later.log", None);
        let events = missing.poll();
        assert!(matches!(&events[..], [TailEvent::Note(n)] if n.contains("does not exist")));
        append(&dir.path().join("later.log"), "hello\n");
        assert_eq!(lines(&missing.poll()), ["hello"]);
    }

    #[test]
    fn globs_tag_lines_with_their_file() {
        let dir = tempfile::tempdir().expect("tempdir");
        append(&dir.path().join("a.log"), "from a\n");
        append(&dir.path().join("b.log"), "from b\n");
        let mut tail = FileTail::new(dir.path(), "*.log", None);
        let events = tail.poll();
        let files: Vec<_> = events
            .iter()
            .filter_map(|e| match e {
                TailEvent::Lines { file, .. } => file.file_name().map(|n| n.to_owned()),
                TailEvent::Note(_) => None,
            })
            .collect();
        assert_eq!(files, ["a.log", "b.log"]);
    }
}

//! Unified diff (`git diff` / `git show -p`) → [`FileDiff`]s.

use ferrumgit_domain::{ChangeKind, DiffLine, FileDiff, Hunk, LineKind};

/// Undoes git's C-style path quoting (`"a\tb\303\251"`). Unquoted input is returned as is.
#[must_use]
pub fn unquote(s: &str) -> String {
    let Some(inner) = s.strip_prefix('"').and_then(|r| r.strip_suffix('"')) else {
        return s.to_owned();
    };
    let mut bytes = Vec::with_capacity(inner.len());
    let mut it = inner.bytes().peekable();
    while let Some(b) = it.next() {
        if b != b'\\' {
            bytes.push(b);
            continue;
        }
        match it.next() {
            Some(b'n') => bytes.push(b'\n'),
            Some(b't') => bytes.push(b'\t'),
            Some(b'r') => bytes.push(b'\r'),
            Some(b'a') => bytes.push(7),
            Some(b'b') => bytes.push(8),
            Some(b'f') => bytes.push(12),
            Some(b'v') => bytes.push(11),
            Some(d @ b'0'..=b'7') => {
                let mut v = u32::from(d - b'0');
                for _ in 0..2 {
                    match it.peek() {
                        Some(&n @ b'0'..=b'7') => {
                            v = v * 8 + u32::from(n - b'0');
                            it.next();
                        }
                        _ => break,
                    }
                }
                bytes.push(u8::try_from(v).unwrap_or(b'?'));
            }
            Some(other) => bytes.push(other),
            None => bytes.push(b'\\'),
        }
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

/// Path from a `---`/`+++` line, without the `a/`/`b/` prefix; `None` for /dev/null.
fn side_path(rest: &str) -> Option<String> {
    let p = unquote(rest.trim_end_matches('\t').trim_end());
    if p == "/dev/null" {
        return None;
    }
    Some(
        p.strip_prefix("a/")
            .or_else(|| p.strip_prefix("b/"))
            .unwrap_or(&p)
            .to_owned(),
    )
}

/// Best-effort path from `diff --git a/X b/Y` (used for binary files and mode-only changes).
fn git_line_path(rest: &str) -> String {
    if rest.starts_with('"') {
        // Quoted: take the second quoted token if present.
        let parts: Vec<&str> = rest.split("\" ").collect();
        let last = parts.last().copied().unwrap_or(rest);
        let last = if last.starts_with('"') {
            last.to_owned()
        } else {
            format!("\"{last}")
        };
        return side_path(&last).unwrap_or_default();
    }
    let n = rest.len();
    if n > 5 && (n - 5).is_multiple_of(2) {
        let p = (n - 5) / 2;
        if rest.get(2..2 + p) == rest.get(n - p..) {
            return rest[n - p..].to_owned();
        }
    }
    rest.split_once(" b/").map_or_else(
        || rest.trim_start_matches("a/").to_owned(),
        |(_, b)| b.to_owned(),
    )
}

fn parse_range(s: &str) -> (u32, u32) {
    let (start, len) = s.split_once(',').unwrap_or((s, "1"));
    (start.parse().unwrap_or(0), len.parse().unwrap_or(0))
}

fn parse_hunk_header(line: &str) -> Option<Hunk> {
    let body = line.strip_prefix("@@ ")?;
    let end = body.find(" @@")?;
    let mut ranges = body[..end].split(' ');
    let (old_start, old_lines) = parse_range(ranges.next()?.strip_prefix('-')?);
    let (new_start, new_lines) = parse_range(ranges.next()?.strip_prefix('+')?);
    Some(Hunk {
        header: line.to_owned(),
        old_start,
        old_lines,
        new_start,
        new_lines,
        lines: Vec::new(),
    })
}

#[derive(Default)]
struct Builder {
    file: Option<FileDiff>,
    old_no: u32,
    new_no: u32,
    minus: Option<String>,
    plus: Option<String>,
}

impl Builder {
    fn finish(&mut self, out: &mut Vec<FileDiff>) {
        if let Some(mut f) = self.file.take() {
            if f.path.is_empty() {
                f.path = self
                    .plus
                    .take()
                    .or_else(|| self.minus.clone())
                    .unwrap_or_default();
            }
            if f.change == ChangeKind::Unmodified {
                f.change = ChangeKind::Modified;
            }
            out.push(f);
        }
        self.minus = None;
        self.plus = None;
    }
}

/// Parses a multi-file patch.
#[must_use]
pub fn parse_diff(raw: &str) -> Vec<FileDiff> {
    let mut out = Vec::new();
    let mut b = Builder::default();
    for line in raw.lines() {
        if let Some(rest) = line.strip_prefix("diff --git ") {
            b.finish(&mut out);
            b.file = Some(FileDiff {
                path: git_line_path(rest),
                header: vec![line.to_owned()],
                ..FileDiff::default()
            });
            continue;
        }
        let Some(f) = b.file.as_mut() else { continue };
        let in_hunk = !f.hunks.is_empty();
        if line.starts_with("@@ ") {
            if let Some(h) = parse_hunk_header(line) {
                b.old_no = h.old_start;
                b.new_no = h.new_start;
                f.hunks.push(h);
            }
            continue;
        }
        if !in_hunk {
            f.header.push(line.to_owned());
            if line.starts_with("new file mode") {
                f.change = ChangeKind::Added;
            } else if line.starts_with("deleted file mode") {
                f.change = ChangeKind::Deleted;
            } else if let Some(p) = line.strip_prefix("rename from ") {
                f.change = ChangeKind::Renamed;
                f.old_path = Some(unquote(p));
            } else if let Some(p) = line.strip_prefix("rename to ") {
                f.path = unquote(p);
            } else if let Some(p) = line.strip_prefix("copy from ") {
                f.change = ChangeKind::Copied;
                f.old_path = Some(unquote(p));
            } else if let Some(p) = line.strip_prefix("copy to ") {
                f.path = unquote(p);
            } else if line.starts_with("Binary files ") || line == "GIT binary patch" {
                f.binary = true;
            } else if let Some(p) = line.strip_prefix("--- ") {
                b.minus = side_path(p);
            } else if let Some(p) = line.strip_prefix("+++ ") {
                b.plus = side_path(p);
                if let Some(p) = b.plus.clone().or_else(|| b.minus.clone())
                    && f.change != ChangeKind::Renamed
                    && f.change != ChangeKind::Copied
                {
                    f.path = p;
                }
            }
            continue;
        }
        let Some(h) = f.hunks.last_mut() else {
            continue;
        };
        let (kind, content) = match line.as_bytes().first() {
            Some(b'+') => (LineKind::Added, &line[1..]),
            Some(b'-') => (LineKind::Removed, &line[1..]),
            Some(b'\\') => (LineKind::NoNewline, line),
            Some(b' ') => (LineKind::Context, &line[1..]),
            // Empty context lines can lose their leading space in some tools.
            None => (LineKind::Context, ""),
            _ => continue,
        };
        let (old_no, new_no) = match kind {
            LineKind::Context => {
                let r = (Some(b.old_no), Some(b.new_no));
                b.old_no += 1;
                b.new_no += 1;
                r
            }
            LineKind::Added => {
                f.additions += 1;
                let r = (None, Some(b.new_no));
                b.new_no += 1;
                r
            }
            LineKind::Removed => {
                f.deletions += 1;
                let r = (Some(b.old_no), None);
                b.old_no += 1;
                r
            }
            LineKind::NoNewline => (None, None),
        };
        h.lines.push(DiffLine {
            kind,
            content: content.to_owned(),
            old_no,
            new_no,
        });
    }
    b.finish(&mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const PATCH: &str = "diff --git a/src/lib.rs b/src/lib.rs
index 1111111..2222222 100644
--- a/src/lib.rs
+++ b/src/lib.rs
@@ -1,4 +1,5 @@ fn main() {
 one
-two
+TWO
+three
 four
\\ No newline at end of file
@@ -10 +11 @@
-x
+y
diff --git a/new.txt b/new.txt
new file mode 100644
index 0000000..3333333
--- /dev/null
+++ b/new.txt
@@ -0,0 +1 @@
+hello
diff --git a/old name.txt b/new name.txt
similarity index 90%
rename from old name.txt
rename to new name.txt
diff --git a/img.png b/img.png
index 4444444..5555555 100644
Binary files a/img.png and b/img.png differ
diff --git a/gone.rs b/gone.rs
deleted file mode 100644
--- a/gone.rs
+++ /dev/null
@@ -1 +0,0 @@
-bye
";

    #[test]
    fn parses_files_hunks_and_kinds() {
        let files = parse_diff(PATCH);
        assert_eq!(files.len(), 5);
        let lib = &files[0];
        assert_eq!(lib.path, "src/lib.rs");
        assert_eq!(lib.change, ChangeKind::Modified);
        assert_eq!(lib.hunks.len(), 2);
        assert_eq!((lib.additions, lib.deletions), (3, 2));
        let h = &lib.hunks[0];
        assert_eq!(
            (h.old_start, h.old_lines, h.new_start, h.new_lines),
            (1, 4, 1, 5)
        );
        assert_eq!(h.lines[1].kind, LineKind::Removed);
        assert_eq!(h.lines[1].old_no, Some(2));
        assert_eq!(h.lines[2].new_no, Some(2));
        assert_eq!(h.lines[4].new_no, Some(4));
        assert_eq!(h.lines[5].kind, LineKind::NoNewline);
        assert_eq!(lib.hunks[1].old_lines, 1);
        assert_eq!(lib.header.len(), 4);

        assert_eq!(files[1].change, ChangeKind::Added);
        assert_eq!(files[1].path, "new.txt");
        assert_eq!(files[2].change, ChangeKind::Renamed);
        assert_eq!(files[2].path, "new name.txt");
        assert_eq!(files[2].old_path.as_deref(), Some("old name.txt"));
        assert!(files[3].binary);
        assert_eq!(files[3].path, "img.png");
        assert_eq!(files[4].change, ChangeKind::Deleted);
        assert_eq!(files[4].path, "gone.rs");
    }

    #[test]
    fn unquotes_c_style_paths() {
        assert_eq!(unquote("\"tab\\there\""), "tab\there");
        assert_eq!(unquote("\"caf\\303\\251.txt\""), "café.txt");
        assert_eq!(unquote("plain.txt"), "plain.txt");
        let f = parse_diff(
            "diff --git \"a/sp ace\\t.txt\" \"b/sp ace\\t.txt\"\nindex 1..2 100644\nBinary files differ\n",
        );
        assert_eq!(f[0].path, "sp ace\t.txt");
    }

    #[test]
    fn binary_path_with_spaces_from_git_line() {
        let f = parse_diff(
            "diff --git a/my file.bin b/my file.bin\nindex 1..2\nBinary files a/my file.bin and b/my file.bin differ\n",
        );
        assert_eq!(f[0].path, "my file.bin");
    }
}

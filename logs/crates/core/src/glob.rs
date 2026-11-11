//! Resolving a file source's path or glob (`*`, `?`, `**`) to files.

use std::path::{Component, Path, PathBuf};

/// At most this many files are followed per source.
pub const MAX_FILES: usize = 64;
/// Directory entries a `**` walk looks at before giving up.
const MAX_WALK: usize = 20_000;

/// The pattern as an absolute path: `~/…` from home, relative from `project`.
#[must_use]
pub fn absolute(project: &Path, pattern: &str, home: Option<&Path>) -> PathBuf {
    let pattern = pattern.trim();
    if let (Some(rest), Some(home)) = (pattern.strip_prefix("~/"), home) {
        return home.join(rest);
    }
    let path = Path::new(pattern);
    if path.is_absolute() {
        path.to_path_buf()
    } else {
        project.join(path)
    }
}

#[must_use]
pub fn has_wildcards(pattern: &str) -> bool {
    pattern.contains(['*', '?'])
}

/// Files matching `pattern` (sorted, at most [`MAX_FILES`]). A pattern
/// without wildcards is returned as is, existing or not, so a log file
/// that appears later is picked up.
#[must_use]
pub fn resolve(project: &Path, pattern: &str, home: Option<&Path>) -> Vec<PathBuf> {
    let full = absolute(project, pattern, home);
    if !has_wildcards(pattern) {
        return vec![full];
    }
    let mut base = PathBuf::new();
    let mut parts: Vec<String> = Vec::new();
    for component in full.components() {
        match component {
            Component::Normal(name) => {
                let name = name.to_string_lossy().into_owned();
                if parts.is_empty() && !has_wildcards(&name) {
                    base.push(&name);
                } else {
                    parts.push(name);
                }
            }
            other if parts.is_empty() => base.push(other.as_os_str()),
            _ => {}
        }
    }
    let mut out = Vec::new();
    let mut budget = MAX_WALK;
    walk(&base, &parts, &mut out, &mut budget);
    out.sort();
    out.dedup();
    out.truncate(MAX_FILES);
    out
}

fn walk(dir: &Path, parts: &[String], out: &mut Vec<PathBuf>, budget: &mut usize) {
    let Some((first, rest)) = parts.split_first() else {
        if dir.is_file() {
            out.push(dir.to_path_buf());
        }
        return;
    };
    if first == "**" {
        // Zero directories, then each subdirectory (skipping hidden ones).
        walk(dir, rest, out, budget);
        for entry in entries(dir, budget) {
            let name = entry.file_name().map(|n| n.to_string_lossy().into_owned());
            if entry.is_dir() && !name.is_some_and(|n| n.starts_with('.')) {
                walk(&entry, parts, out, budget);
            }
        }
        return;
    }
    if !has_wildcards(first) {
        walk(&dir.join(first), rest, out, budget);
        return;
    }
    for entry in entries(dir, budget) {
        let Some(name) = entry.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            continue;
        };
        if wildcard_match(first, &name) {
            walk(&entry, rest, out, budget);
        }
    }
}

fn entries(dir: &Path, budget: &mut usize) -> Vec<PathBuf> {
    let Ok(read) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for entry in read.flatten() {
        if *budget == 0 {
            break;
        }
        *budget -= 1;
        out.push(entry.path());
    }
    out
}

/// `*` (any run) and `?` (one character) within a single path segment.
#[must_use]
pub fn wildcard_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let n: Vec<char> = name.chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|&c| c == '*')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wildcards() {
        assert!(wildcard_match("*.log", "app.log"));
        assert!(wildcard_match("app-?.log", "app-1.log"));
        assert!(wildcard_match("*", ""));
        assert!(!wildcard_match("*.log", "app.log.1"));
        assert!(wildcard_match("*.log*", "app.log.1"));
        assert!(!wildcard_match("a?c", "ac"));
    }

    #[test]
    fn resolves_relative_absolute_and_recursive() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        for file in [
            "logs/a.log",
            "logs/b.txt",
            "logs/old/c.log",
            "storage/logs/l.log",
        ] {
            let path = root.join(file);
            std::fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
            std::fs::write(&path, "x").expect("write");
        }
        let names = |pattern: &str| -> Vec<String> {
            resolve(root, pattern, None)
                .iter()
                .map(|p| {
                    p.strip_prefix(root)
                        .expect("inside")
                        .to_string_lossy()
                        .into_owned()
                })
                .collect()
        };
        assert_eq!(names("logs/*.log"), ["logs/a.log"]);
        assert_eq!(names("logs/**/*.log"), ["logs/a.log", "logs/old/c.log"]);
        assert_eq!(names("*/logs/*.log"), ["storage/logs/l.log"]);
        assert_eq!(names("missing.log"), ["missing.log"], "kept for later");
        assert!(names("nothing/*.log").is_empty());
        let absolute_pattern = format!("{}/logs/*.txt", root.display());
        assert_eq!(names(&absolute_pattern), ["logs/b.txt"]);
        assert_eq!(
            absolute(Path::new("/p"), "~/x.log", Some(Path::new("/home/me"))),
            Path::new("/home/me/x.log")
        );
    }
}

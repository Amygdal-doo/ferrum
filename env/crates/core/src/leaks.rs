//! Finding a project's stored secret values in its folder: the working
//! tree (files git would track: `.gitignore`d files such as `.env` are
//! skipped) and every line ever added in its git history.
//!
//! The scanner never holds the values. [`Fingerprints`] keeps, per value,
//! its length, a 64-bit rolling hash (to find candidates quickly) and its
//! SHA-256 (to confirm them); findings name the file, the commit and the
//! variable, never the value. Only variables marked secret with values of
//! at least [`MIN_LEN`] bytes are looked for, so `PORT=8080` isn't noise.

use std::collections::{BTreeMap, HashMap};
use std::io::{BufRead, BufReader};
use std::path::Path;
use std::process::{Command, Stdio};

use sha2::{Digest, Sha256};

use crate::model::Project;
use crate::{Error, Result};

/// Shorter values are not looked for.
pub const MIN_LEN: usize = 8;
/// Larger files in the working tree are skipped.
const MAX_FILE_BYTES: u64 = 4 * 1024 * 1024;
const BASE: u64 = 0x0100_0000_01b3;

/// What to do about a finding (shown with the results; nothing is
/// rewritten automatically).
pub const GUIDANCE: &str = "\
1. Treat each value found as exposed: rotate it with its provider, then store the new one here.
2. Remove it from the file and read it from the environment instead (fenv run, or a .env file listed in .gitignore).
3. If it was committed: rewriting history (git filter-repo, BFG) and force-pushing only helps after rotating, and clones, forks and CI caches keep the old commits.";

/// A variable a value belongs to.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Label {
    pub environment: String,
    pub key: String,
}

/// Lengths, rolling hashes and SHA-256 digests of the values to look for.
#[derive(Default)]
pub struct Fingerprints {
    /// length → rolling hash → digests of that length and hash.
    candidates: BTreeMap<usize, HashMap<u64, Vec<[u8; 32]>>>,
    labels: HashMap<[u8; 32], Vec<Label>>,
}

fn rolling(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0u64, |h, &b| {
        h.wrapping_mul(BASE).wrapping_add(u64::from(b))
    })
}

impl Fingerprints {
    /// The secret values of every environment of `project`.
    #[must_use]
    pub fn new(project: &Project) -> Self {
        let mut out = Self::default();
        for env in &project.environments {
            for var in env.variables.iter().filter(|v| v.secret) {
                let value = var.value.as_bytes();
                if value.len() < MIN_LEN {
                    continue;
                }
                let digest: [u8; 32] = Sha256::digest(value).into();
                let slot = out
                    .candidates
                    .entry(value.len())
                    .or_default()
                    .entry(rolling(value))
                    .or_default();
                if !slot.contains(&digest) {
                    slot.push(digest);
                }
                out.labels.entry(digest).or_default().push(Label {
                    environment: env.name.clone(),
                    key: var.key.clone(),
                });
            }
        }
        out
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.labels.is_empty()
    }

    /// How many distinct values are looked for.
    #[must_use]
    pub fn len(&self) -> usize {
        self.labels.len()
    }

    /// Every stored value found in `text`, with the byte offset of its
    /// first occurrence.
    #[must_use]
    pub fn find(&self, text: &[u8]) -> Vec<(usize, &[Label])> {
        let mut found: Vec<(usize, &[Label])> = Vec::new();
        for (&len, hashes) in &self.candidates {
            if text.len() < len {
                break;
            }
            let top = (0..len - 1).fold(1u64, |p, _| p.wrapping_mul(BASE));
            let mut h = rolling(&text[..len]);
            for start in 0..=text.len() - len {
                if start > 0 {
                    let out = u64::from(text[start - 1]).wrapping_mul(top);
                    h = h
                        .wrapping_sub(out)
                        .wrapping_mul(BASE)
                        .wrapping_add(u64::from(text[start + len - 1]));
                }
                let Some(digests) = hashes.get(&h) else {
                    continue;
                };
                let digest: [u8; 32] = Sha256::digest(&text[start..start + len]).into();
                if digests.contains(&digest)
                    && let Some(labels) = self.labels.get(&digest)
                    && !found.iter().any(|(_, l)| l.as_ptr() == labels.as_ptr())
                {
                    found.push((start, labels));
                }
            }
        }
        found
    }
}

/// Where a value was found.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct Finding {
    /// Relative to the project folder.
    pub path: String,
    /// 1-based, in the working tree file.
    pub line: Option<usize>,
    /// The (oldest scanned) commit that added it; `None` for the working
    /// tree.
    pub commit: Option<String>,
    pub label: Label,
}

/// The project's files that git would track (or, outside git, every file
/// not ignored), skipping `.git`, binaries and files over 4 MiB.
pub fn scan_working_tree(dir: &Path, prints: &Fingerprints) -> Vec<Finding> {
    let mut findings = Vec::new();
    let walker = ignore::WalkBuilder::new(dir)
        .hidden(false)
        .require_git(false)
        .filter_entry(|e| e.file_name() != ".git")
        .build();
    for entry in walker.filter_map(std::result::Result::ok) {
        if !entry.file_type().is_some_and(|t| t.is_file())
            || entry.metadata().is_ok_and(|m| m.len() > MAX_FILE_BYTES)
        {
            continue;
        }
        let Ok(bytes) = std::fs::read(entry.path()) else {
            continue;
        };
        if bytes[..bytes.len().min(8000)].contains(&0) {
            continue;
        }
        let path = entry
            .path()
            .strip_prefix(dir)
            .unwrap_or(entry.path())
            .to_string_lossy()
            .into_owned();
        for (offset, labels) in prints.find(&bytes) {
            let line = bytes[..offset].iter().filter(|&&b| b == b'\n').count() + 1;
            for label in labels {
                findings.push(Finding {
                    path: path.clone(),
                    line: Some(line),
                    commit: None,
                    label: label.clone(),
                });
            }
        }
    }
    findings.sort();
    findings
}

/// Every line added under `dir` in any commit reachable from any ref (`git log --all
/// -p`). External diff and textconv drivers are disabled, so a repository's
/// config can't make this run its programs.
pub fn scan_history(dir: &Path, prints: &Fingerprints) -> Result<Vec<Finding>> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args([
            "-c",
            "core.quotepath=off",
            "log",
            "--all",
            "-p",
            "--no-color",
            "--no-ext-diff",
            "--no-textconv",
            "--unified=0",
            "--relative",
            "--format=%x00%H",
            "--",
            ".",
        ])
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| Error::Invalid(format!("Could not run git: {e}")))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| Error::Invalid("git produced no output".into()))?;
    // (label, path) → the oldest commit seen adding it (log runs newest
    // first, so later lines overwrite).
    let mut found: BTreeMap<(Label, String), String> = BTreeMap::new();
    let (mut commit, mut path) = (String::new(), None::<String>);
    let mut after_old_header = false;
    let mut reader = BufReader::new(stdout);
    let mut line = Vec::new();
    loop {
        line.clear();
        if reader.read_until(b'\n', &mut line)? == 0 {
            break;
        }
        let text = line.strip_suffix(b"\n").unwrap_or(&line);
        if let Some(hash) = text.strip_prefix(b"\0") {
            commit = String::from_utf8_lossy(hash).into_owned();
            path = None;
        } else if text.starts_with(b"--- ") {
            after_old_header = true;
            continue;
        } else if after_old_header && text.starts_with(b"+++ ") {
            let target = String::from_utf8_lossy(&text[4..]).into_owned();
            path = target.strip_prefix("b/").map(str::to_string);
        } else if let (Some(added), Some(file)) = (text.strip_prefix(b"+"), &path) {
            for (_, labels) in prints.find(added) {
                for label in labels {
                    found.insert((label.clone(), file.clone()), commit.clone());
                }
            }
        }
        after_old_header = false;
    }
    let status = child.wait()?;
    if !status.success() {
        let mut message = String::new();
        if let Some(mut stderr) = child.stderr.take() {
            use std::io::Read as _;
            stderr.read_to_string(&mut message).ok();
        }
        return Err(Error::Invalid(format!(
            "git log failed: {}",
            message.lines().next().unwrap_or("not a git repository?")
        )));
    }
    let mut findings: Vec<Finding> = found
        .into_iter()
        .map(|((label, path), commit)| Finding {
            path,
            line: None,
            commit: Some(commit.chars().take(12).collect()),
            label,
        })
        .collect();
    findings.sort();
    Ok(findings)
}

/// `dir` is (inside) a git work tree.
#[must_use]
pub fn is_git_repo(dir: &Path) -> bool {
    dir.ancestors().any(|d| d.join(".git").exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Variable;

    fn project() -> Project {
        let mut p = Project::new("shop", None);
        let dev = p.environment_mut("development");
        dev.upsert(Variable::new("STRIPE_KEY", "sk_live_abcdef123"));
        dev.upsert(Variable::new("SHORT", "abc"));
        let mut public = Variable::new("PUBLIC_URL", "https://shop.example");
        public.secret = false;
        dev.upsert(public);
        p.environment_mut("production")
            .upsert(Variable::new("DB_PASSWORD", "hunter2-hunter2"));
        // Same value in two environments.
        p.environment_mut("staging")
            .upsert(Variable::new("STRIPE_KEY", "sk_live_abcdef123"));
        p
    }

    #[test]
    fn fingerprints_find_values_without_holding_them() {
        let prints = Fingerprints::new(&project());
        assert_eq!(prints.len(), 2, "short and non-secret values are skipped");
        let text = b"config = { key: \"sk_live_abcdef123\" }\npw=hunter2-hunter2";
        let found = prints.find(text);
        let mut names: Vec<String> = found
            .iter()
            .flat_map(|(_, labels)| labels.iter())
            .map(|l| format!("{}/{}", l.environment, l.key))
            .collect();
        names.sort();
        assert_eq!(
            names,
            [
                "development/STRIPE_KEY",
                "production/DB_PASSWORD",
                "staging/STRIPE_KEY"
            ]
        );
        assert_eq!(found.iter().map(|(o, _)| *o).min(), Some(17));
        assert!(prints.find(b"sk_live_abcdef12 almost").is_empty());
        assert!(prints.find(b"short").is_empty());
        assert!(prints.find(b"https://shop.example").is_empty());
        assert!(Fingerprints::new(&Project::new("empty", None)).is_empty());
    }

    #[test]
    fn working_tree_skips_ignored_and_binary_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        std::fs::write(root.join(".gitignore"), ".env\n").expect("write");
        std::fs::write(root.join(".env"), "DB_PASSWORD=hunter2-hunter2\n").expect("write");
        std::fs::create_dir(root.join("src")).expect("dir");
        std::fs::write(
            root.join("src/config.js"),
            "// config\n\nconst db = 'hunter2-hunter2';\n",
        )
        .expect("write");
        std::fs::write(root.join("blob.bin"), b"\0\0hunter2-hunter2").expect("write");
        let findings = scan_working_tree(root, &Fingerprints::new(&project()));
        assert_eq!(findings.len(), 1, "{findings:?}");
        let f = &findings[0];
        assert_eq!(f.path, "src/config.js");
        assert_eq!(f.line, Some(3));
        assert_eq!(f.label.key, "DB_PASSWORD");
        assert!(f.commit.is_none());
        assert!(!format!("{findings:?}").contains("hunter2"));
    }

    fn git(dir: &Path, args: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
            .args(["-c", "commit.gpgsign=false"])
            .args(args)
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git");
        assert!(status.success(), "git {args:?}");
    }

    #[test]
    fn history_finds_removed_values() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let dir = tempfile::tempdir().expect("tempdir");
        let root = dir.path();
        git(root, &["init", "-q"]);
        std::fs::write(root.join("app.py"), "KEY = 'sk_live_abcdef123'\n").expect("write");
        git(root, &["add", "."]);
        git(root, &["commit", "-q", "-m", "oops"]);
        std::fs::write(root.join("app.py"), "KEY = os.environ['KEY']\n").expect("write");
        git(root, &["commit", "-q", "-am", "fix"]);
        let prints = Fingerprints::new(&project());
        assert!(scan_working_tree(root, &prints).is_empty());
        let findings = scan_history(root, &prints).expect("history");
        let keys: Vec<_> = findings
            .iter()
            .map(|f| (f.path.as_str(), f.label.environment.as_str()))
            .collect();
        assert_eq!(keys, [("app.py", "development"), ("app.py", "staging")]);
        assert_eq!(findings[0].commit.as_ref().map(String::len), Some(12));
        assert!(is_git_repo(root));
        let plain = tempfile::tempdir().expect("tempdir");
        assert!(scan_history(plain.path(), &prints).is_err());
    }
}

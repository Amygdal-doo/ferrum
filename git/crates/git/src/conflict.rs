//! Conflicted files during a merge, rebase, cherry-pick or revert: both
//! sides are read from the index stages (`:1:` base, `:2:` ours, `:3:`
//! theirs) and merged again with `git merge-file --diff3` into a scratch
//! copy, so the working-tree file (maybe half edited) is not touched.

use crate::error::{GitError, GitResult};
use crate::read::Repo;
use ferrumgit_domain::{ConflictFile, PARSE_MARKER_SIZE};
use std::collections::HashMap;

/// Rejects paths that are empty, absolute, option-like or leave the work tree.
pub(crate) fn check_file_path(path: &str) -> GitResult<()> {
    let p = std::path::Path::new(path);
    if path.is_empty()
        || path.starts_with('-')
        || p.is_absolute()
        || p.components()
            .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return Err(GitError::Invalid(format!("invalid path: {path:?}")));
    }
    Ok(())
}

impl Repo {
    /// Both sides of a conflicted text file, split into common parts and conflicts.
    pub async fn conflict_file(&self, path: &str) -> GitResult<ConflictFile> {
        check_file_path(path)?;
        let out = self
            .run(self.cmd(["ls-files", "-u", "-z", "--", path]))
            .await?;
        // `<mode> <oid> <stage>\t<path>` per stage.
        let stages: HashMap<u8, String> = out
            .stdout_str()
            .split('\0')
            .filter_map(|rec| {
                let (meta, _) = rec.split_once('\t')?;
                let mut f = meta.split_whitespace();
                let (_, oid, stage) = (f.next()?, f.next()?, f.next()?);
                Some((stage.parse().ok()?, oid.to_owned()))
            })
            .collect();
        if stages.is_empty() {
            return Err(GitError::Invalid(format!("{path} is not in conflict")));
        }
        let (Some(ours), Some(theirs)) = (stages.get(&2), stages.get(&3)) else {
            return Err(GitError::Invalid(format!(
                "{path} was deleted on one side: keep ours or theirs instead"
            )));
        };
        let (base, ours, theirs) = (
            self.blob(stages.get(&1)).await?,
            self.blob(Some(ours)).await?,
            self.blob(Some(theirs)).await?,
        );
        if [&base, &ours, &theirs].iter().any(|b| b.contains(&0)) {
            return Err(GitError::Invalid(format!(
                "{path} is a binary file: keep ours or theirs instead"
            )));
        }

        let dir = tempfile::tempdir().map_err(|e| GitError::Spawn(e.to_string()))?;
        let io = |e: std::io::Error| GitError::Spawn(e.to_string());
        let file = |name: &str, bytes: &[u8]| -> GitResult<String> {
            let p = dir.path().join(name);
            std::fs::write(&p, bytes).map_err(io)?;
            Ok(p.to_string_lossy().into_owned())
        };
        let (o, b, t) = (
            file("ours", &ours)?,
            file("base", &base)?,
            file("theirs", &theirs)?,
        );
        // Exit status is the number of conflicts (capped at 127).
        let ok: Vec<i32> = (1..=127).collect();
        let merged = self
            .run(
                self.cmd([
                    "merge-file".to_owned(),
                    "-p".to_owned(),
                    "--diff3".to_owned(),
                    format!("--marker-size={PARSE_MARKER_SIZE}"),
                    "-L".to_owned(),
                    "ours".to_owned(),
                    "-L".to_owned(),
                    "base".to_owned(),
                    "-L".to_owned(),
                    "theirs".to_owned(),
                    "--".to_owned(),
                    o,
                    b,
                    t,
                ])
                .ok_codes(&ok),
            )
            .await?;
        Ok(ConflictFile::parse(path, &merged.stdout_str()))
    }

    /// A blob's bytes; nothing for a missing stage (added on both sides).
    async fn blob(&self, oid: Option<&String>) -> GitResult<Vec<u8>> {
        match oid {
            None => Ok(Vec::new()),
            Some(oid) => Ok(self
                .run(self.cmd(["cat-file", "blob", oid.as_str()]))
                .await?
                .stdout),
        }
    }

    /// Writes `content` to the work-tree file `path`.
    pub async fn write_file(&self, path: &str, content: &str) -> GitResult<()> {
        check_file_path(path)?;
        tokio::fs::write(self.root.join(path), content)
            .await
            .map_err(|e| GitError::Spawn(e.to_string()))
    }

    /// Writes `content` to `path` and stages it (marks the conflict resolved).
    pub(crate) async fn resolve(&self, path: &str, content: &str) -> GitResult<crate::Output> {
        self.write_file(path, content).await?;
        self.run(self.cmd(["add", "--", path])).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_paths_outside_the_work_tree() {
        assert!(check_file_path("src/a.rs").is_ok());
        for bad in ["", "-x", "/etc/passwd", "../x", "a/../../x"] {
            assert!(check_file_path(bad).is_err(), "{bad}");
        }
    }
}

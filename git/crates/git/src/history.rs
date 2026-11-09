//! File history, blame, message search and reflog-based undo.

use crate::error::{GitError, GitResult};
use crate::parse::{self, FILE_LOG_FORMAT, REFLOG_FORMAT};
use crate::read::{Repo, check_rev};
use ferrumgit_domain::{Blame, FileCommit, Oid, Undo};

/// Rejects empty paths and paths git would read as options.
fn check_path(path: &str) -> GitResult<()> {
    if path.is_empty() || path.starts_with('-') {
        return Err(GitError::Invalid(format!("invalid path: {path:?}")));
    }
    Ok(())
}

impl Repo {
    /// Up to `limit` commits touching `path`, newest first, following renames.
    /// Starts at `rev` (default HEAD).
    pub async fn file_history(
        &self,
        path: &str,
        rev: Option<&str>,
        limit: usize,
    ) -> GitResult<Vec<FileCommit>> {
        check_path(path)?;
        if rev.is_none() && !self.head_exists().await {
            return Ok(Vec::new());
        }
        let mut c = self.cmd([
            "log".to_owned(),
            "--follow".to_owned(),
            "-M".to_owned(),
            "--name-status".to_owned(),
            "-z".to_owned(),
            format!("--max-count={limit}"),
            format!("--format={FILE_LOG_FORMAT}"),
        ]);
        if let Some(r) = rev {
            check_rev(r)?;
            c = c.arg(r);
        }
        let out = self.run(c.args(["--", path])).await?;
        Ok(parse::parse_file_log(&out.stdout, path))
    }

    /// Who last changed each line of `path` at `rev`, or in the working tree
    /// (uncommitted lines get the zero id).
    pub async fn blame(&self, path: &str, rev: Option<&str>) -> GitResult<Blame> {
        check_path(path)?;
        let mut c = self.cmd(["blame", "--porcelain"]);
        if let Some(r) = rev {
            check_rev(r)?;
            c = c.arg(r);
        }
        let out = self.run(c.args(["--", path])).await?;
        Ok(parse::parse_blame(&out.stdout, path))
    }

    /// Full ids of up to `limit` commits (all refs, newest first) whose whole
    /// message contains `text`, case-insensitively.
    pub async fn grep_messages(&self, text: &str, limit: usize) -> GitResult<Vec<String>> {
        let text = text.trim();
        if text.is_empty() || !self.head_exists().await {
            return Ok(Vec::new());
        }
        let out = self
            .run(self.cmd([
                "log".to_owned(),
                "--exclude=refs/stash".to_owned(),
                "--all".to_owned(),
                "-i".to_owned(),
                "--fixed-strings".to_owned(),
                format!("--grep={text}"),
                format!("--max-count={limit}"),
                "--format=%H".to_owned(),
            ]))
            .await?;
        Ok(out
            .stdout_str()
            .lines()
            .filter(|l| l.len() >= 40)
            .map(str::to_owned)
            .collect())
    }

    /// The last local operation if it can be undone safely right now: a
    /// commit / amend that has not been pushed, or a checkout.
    pub async fn undo_plan(&self) -> GitResult<Option<Undo>> {
        if self.operation().is_some() {
            return Ok(None);
        }
        let Ok(out) = self
            .run(self.cmd([
                "reflog".to_owned(),
                "-z".to_owned(),
                "-n".to_owned(),
                "2".to_owned(),
                format!("--format={REFLOG_FORMAT}"),
                "HEAD".to_owned(),
            ]))
            .await
        else {
            // No reflog (no commits yet, or logging disabled).
            return Ok(None);
        };
        let Some(plan) = Undo::from_reflog(&parse::parse_reflog(&out.stdout)) else {
            return Ok(None);
        };
        if self.rev(&["HEAD"]).await.as_ref() != Some(plan.head()) {
            return Ok(None);
        }
        let safe = match &plan {
            Undo::Commit { to, amend, .. } => {
                let parent_ok = *amend || self.rev(&["HEAD^"]).await.as_ref() == Some(to);
                parent_ok && !self.head_is_pushed().await
            }
            Undo::Checkout { from, .. } => {
                check_rev(from).is_ok()
                    && self.rev(&[&format!("{from}^{{commit}}")]).await.is_some()
            }
        };
        Ok(safe.then_some(plan))
    }

    /// Runs `undo` if it is still the current plan.
    pub(crate) async fn undo(&self, undo: &Undo) -> GitResult<crate::Output> {
        if self.undo_plan().await?.as_ref() != Some(undo) {
            return Err(GitError::Invalid(
                "the repository changed; there is nothing to undo safely".into(),
            ));
        }
        match undo {
            Undo::Commit { to, .. } => {
                check_rev(to.as_str())?;
                self.run(self.cmd(["reset", "-q", "--soft", to.as_str()]))
                    .await
            }
            Undo::Checkout { from, .. } => {
                check_rev(from)?;
                let branch = format!("refs/heads/{from}");
                if self.rev(&["--verify", "-q", &branch]).await.is_some() {
                    self.run(self.cmd(["switch", "--no-guess", from])).await
                } else {
                    self.run(self.cmd(["switch", "--detach", from])).await
                }
            }
        }
    }

    /// `git rev-parse <args>` as an id; `None` if it does not resolve.
    async fn rev(&self, args: &[&str]) -> Option<Oid> {
        let out = self
            .run(self.cmd(std::iter::once("rev-parse").chain(args.iter().copied())))
            .await
            .ok()?;
        let s = out.stdout_str();
        let s = s.trim();
        (!s.is_empty()).then(|| Oid::new(s))
    }

    /// HEAD is already on its branch's upstream (undoing would rewrite pushed history).
    async fn head_is_pushed(&self) -> bool {
        self.run(
            self.cmd(["merge-base", "--is-ancestor", "HEAD", "@{upstream}"])
                .ok_codes(&[1]),
        )
        .await
        .is_ok_and(|o| o.code == 0)
    }
}

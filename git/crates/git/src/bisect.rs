//! `git bisect` start / good / bad / skip / reset, and the session's state
//! for the banner (from `BISECT_START`, `BISECT_LOG` and `refs/bisect/*`).

use crate::error::{GitError, GitResult};
use crate::read::{Repo, check_rev};
use ferrumgit_domain::{BisectMark, BisectState, Oid, found_in_log};

impl Repo {
    /// `true` while a bisect session is active.
    #[must_use]
    pub fn bisecting(&self) -> bool {
        self.git_dir.join("BISECT_START").is_file()
    }

    /// The session in progress, if any.
    pub async fn bisect_state(&self) -> GitResult<Option<BisectState>> {
        if !self.bisecting() {
            return Ok(None);
        }
        let read =
            |name: &str| std::fs::read_to_string(self.git_dir.join(name)).unwrap_or_default();
        let refs = self
            .run(self.cmd([
                "for-each-ref",
                "--format=%(refname) %(objectname)",
                "refs/bisect",
            ]))
            .await?
            .stdout_str();
        let mut state = BisectState {
            start: read("BISECT_START").trim().to_owned(),
            found: found_in_log(&read("BISECT_LOG")),
            ..BisectState::default()
        };
        let mut good = Vec::new();
        for line in refs.lines() {
            let Some((name, oid)) = line.split_once(' ') else {
                continue;
            };
            match name.strip_prefix("refs/bisect/") {
                Some("bad") => state.bad = Some(Oid::new(oid)),
                Some(n) if n.starts_with("good-") => good.push(oid.to_owned()),
                Some(n) if n.starts_with("skip-") => state.skipped += 1,
                _ => {}
            }
        }
        state.good = good.len();
        if state.found.is_some() {
            state.remaining = Some(1);
        } else if let Some(bad) = &state.bad
            && !good.is_empty()
        {
            let mut c = self.cmd(["rev-list", "--count", bad.as_str(), "--not"]);
            c = c.args(&good);
            let count = self.run(c).await?.stdout_str();
            state.remaining = count.trim().parse().ok();
        }
        Ok(Some(state))
    }

    /// Starts a session; marks `bad` and `good` when given. Returns git's
    /// progress line ("Bisecting: …") or the first bad commit.
    pub(crate) async fn bisect_start(
        &self,
        bad: Option<&Oid>,
        good: Option<&Oid>,
    ) -> GitResult<Option<String>> {
        if self.bisecting() {
            return Err(GitError::Invalid(
                "a bisect is already in progress; reset it first".into(),
            ));
        }
        if let Some(op) = self.operation() {
            return Err(GitError::Invalid(format!(
                "finish the {} in progress first",
                op.label().to_lowercase()
            )));
        }
        for rev in bad.iter().chain(good.iter()) {
            check_rev(rev.as_str())?;
        }
        let out = self.run(self.cmd(["bisect", "start"])).await?;
        let mut message = out.summary();
        let marks = [(BisectMark::Bad, bad), (BisectMark::Good, good)];
        for (mark, rev) in marks {
            if let Some(rev) = rev {
                message = match self.bisect_mark(mark, Some(rev)).await {
                    Ok(m) => m,
                    Err(e) => {
                        // Leave no half-started session behind.
                        let _ = self.run(self.cmd(["bisect", "reset"])).await;
                        return Err(e);
                    }
                };
            }
        }
        Ok(message)
    }

    /// Marks `rev` (default: HEAD) good, bad or skipped; git checks out the
    /// next commit to test.
    pub(crate) async fn bisect_mark(
        &self,
        mark: BisectMark,
        rev: Option<&Oid>,
    ) -> GitResult<Option<String>> {
        if !self.bisecting() {
            return Err(GitError::Invalid("no bisect in progress".into()));
        }
        let mut c = self.cmd(["bisect", mark.command()]);
        if let Some(rev) = rev {
            check_rev(rev.as_str())?;
            c = c.arg(rev.as_str());
        }
        let out = self.run(c).await?;
        Ok(progress_line(&out.stdout_str()).or_else(|| out.summary()))
    }

    pub(crate) async fn bisect_reset(&self) -> GitResult<Option<String>> {
        if !self.bisecting() {
            return Err(GitError::Invalid("no bisect in progress".into()));
        }
        Ok(self.run(self.cmd(["bisect", "reset"])).await?.summary())
    }
}

/// "<oid> is the first bad commit" → "First bad commit: <short>"; else
/// git's "Bisecting: N revisions left…" line.
fn progress_line(stdout: &str) -> Option<String> {
    if let Some(line) = stdout
        .lines()
        .find(|l| l.contains("is the first bad commit"))
    {
        let oid = line.split_whitespace().next().unwrap_or_default();
        return Some(format!("First bad commit: {}", oid.get(..7).unwrap_or(oid)));
    }
    stdout
        .lines()
        .find(|l| l.starts_with("Bisecting:"))
        .map(str::to_owned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_progress_and_result_lines() {
        assert_eq!(
            progress_line(
                "Bisecting: 3 revisions left to test after this (roughly 2 steps)\n[abc] x\n"
            )
            .as_deref(),
            Some("Bisecting: 3 revisions left to test after this (roughly 2 steps)")
        );
        assert_eq!(
            progress_line("1234567890abcdef is the first bad commit\ncommit 1234…\n").as_deref(),
            Some("First bad commit: 1234567")
        );
        assert_eq!(progress_line("nothing"), None);
    }
}

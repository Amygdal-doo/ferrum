//! Pull request refs: fetch a forge's PR head into a private namespace and
//! diff it against the merge base with the target branch.

use crate::error::{GitError, GitResult};
use crate::read::{Repo, check_rev};
use crate::ssh::NetEnv;
use ferrumgit_domain::FileDiff;
use tokio_util::sync::CancellationToken;

/// Private ref namespace for fetched pull requests; hidden from the graph.
pub const PR_NAMESPACE: &str = "refs/ferrumgit/";

fn check_refspec(spec: &str) -> GitResult<()> {
    let s = spec.trim_start_matches('+');
    if s.is_empty() || s.starts_with('-') || s.contains(char::is_whitespace) || s.contains("..") {
        return Err(GitError::Invalid(format!("invalid refspec: {spec:?}")));
    }
    Ok(())
}

impl Repo {
    /// `git fetch --no-tags <remote> <refspecs…>` with the profile's SSH environment.
    pub async fn fetch_refspecs(
        &self,
        remote: &str,
        refspecs: &[String],
        net: Option<NetEnv>,
        cancel: Option<CancellationToken>,
    ) -> GitResult<()> {
        if remote.is_empty() || remote.starts_with('-') || remote.contains(char::is_whitespace) {
            return Err(GitError::Invalid(format!(
                "invalid remote name: {remote:?}"
            )));
        }
        for spec in refspecs {
            check_refspec(spec)?;
        }
        let cmd = self
            .cmd([
                "fetch",
                "--no-tags",
                "--no-write-fetch-head",
                "--quiet",
                remote,
            ])
            .args(refspecs)
            .net(net)
            .cancel(cancel);
        self.run(cmd).await.map(|_| ())
    }

    /// Best common ancestor of two revisions.
    pub async fn merge_base(&self, a: &str, b: &str) -> GitResult<String> {
        check_rev(a)?;
        check_rev(b)?;
        let out = self.run(self.cmd(["merge-base", a, b])).await?;
        let oid = out.stdout_str().trim().to_owned();
        if oid.is_empty() {
            return Err(GitError::Invalid(format!(
                "{a} and {b} have no common history"
            )));
        }
        Ok(oid)
    }

    /// Changes from `from` to `to` (renames detected). The flag is `true` when
    /// the patch was cut at [`crate::MAX_PATCH_BYTES`].
    pub async fn diff_range(&self, from: &str, to: &str) -> GitResult<(Vec<FileDiff>, bool)> {
        check_rev(from)?;
        check_rev(to)?;
        let out = self
            .run(self.cmd([
                "diff",
                "--no-color",
                "--no-ext-diff",
                "-M",
                "-U3",
                from,
                to,
                "--",
            ]))
            .await?;
        Ok(crate::read::parse_limited(&out.stdout))
    }

    /// Full object id of `rev`.
    pub async fn rev_parse(&self, rev: &str) -> GitResult<String> {
        check_rev(rev)?;
        let out = self
            .run(self.cmd(["rev-parse", "--verify", "-q", &format!("{rev}^{{commit}}")]))
            .await?;
        Ok(out.stdout_str().trim().to_owned())
    }

    /// `(remote, url)` as written in git config, before `insteadOf` rewriting
    /// (`git remote -v` shows rewritten URLs).
    pub async fn configured_remote_urls(&self) -> GitResult<Vec<(String, String)>> {
        let out = self
            .run(
                self.cmd(["config", "--get-regexp", r"^remote\..*\.url$"])
                    .ok_codes(&[1]),
            )
            .await?;
        Ok(parse_remote_urls(&out.stdout_str()))
    }

    /// Fast-forwards the checked-out branch to `rev` (fails if it has diverged).
    pub async fn merge_ff_only(&self, rev: &str) -> GitResult<()> {
        check_rev(rev)?;
        self.run(self.cmd(["merge", "--ff-only", "--quiet", rev]))
            .await
            .map(|_| ())
    }
}

fn parse_remote_urls(text: &str) -> Vec<(String, String)> {
    text.lines()
        .filter_map(|l| {
            let (key, url) = l.split_once(' ')?;
            let name = key.strip_prefix("remote.")?.strip_suffix(".url")?;
            Some((name.to_owned(), url.trim().to_owned()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{check_refspec, parse_remote_urls};

    #[test]
    fn parses_configured_remote_urls() {
        assert_eq!(
            parse_remote_urls(
                "remote.origin.url https://github.com/o/r.git\nremote.my.fork.url git@h:x/y\n"
            ),
            [
                ("origin".to_owned(), "https://github.com/o/r.git".to_owned()),
                ("my.fork".to_owned(), "git@h:x/y".to_owned())
            ]
        );
    }

    #[test]
    fn validates_refspecs() {
        assert!(check_refspec("+refs/pull/1/head:refs/ferrumgit/pr/1/head").is_ok());
        assert!(check_refspec("-upload-pack=x").is_err());
        assert!(check_refspec("a b").is_err());
        assert!(check_refspec("+").is_err());
    }
}

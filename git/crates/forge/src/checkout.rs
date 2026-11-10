//! Where pull request refs land locally and how "check out" maps to branches.

use crate::model::PullRequest;

/// Private ref holding a fetched PR head (excluded from the commit graph).
#[must_use]
pub fn local_head_ref(number: u64) -> String {
    format!("refs/ferrumgit/pr/{number}/head")
}

/// Private ref holding the fetched base branch tip of a PR.
#[must_use]
pub fn local_base_ref(number: u64) -> String {
    format!("refs/ferrumgit/pr/{number}/base")
}

/// How to check out a pull request locally.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CheckoutPlan {
    /// The head branch lives in the project itself: fetch it and track
    /// `<remote>/<branch>` with a local branch of the same name.
    Track {
        remote_branch: String,
        local: String,
        /// A local branch with that name exists; switch to it and fast-forward.
        exists: bool,
    },
    /// The head is in a fork: create (or fast-forward) `local` at the fetched PR ref.
    Fork { local: String, exists: bool },
}

impl CheckoutPlan {
    #[must_use]
    pub fn local(&self) -> &str {
        match self {
            Self::Track { local, .. } | Self::Fork { local, .. } => local,
        }
    }
}

/// Picks the plan; `local_branches` are short names of existing local branches.
#[must_use]
pub fn checkout_plan(pr: &PullRequest, remote: &str, local_branches: &[String]) -> CheckoutPlan {
    let head = sanitize_branch(&pr.head_branch);
    if !pr.from_fork && !head.is_empty() && head == pr.head_branch {
        let exists = local_branches.contains(&head);
        return CheckoutPlan::Track {
            remote_branch: format!("{remote}/{head}"),
            local: head,
            exists,
        };
    }
    let suffix = head.replace('/', "-");
    let mut local = if suffix.is_empty() {
        format!("pr-{}", pr.number)
    } else {
        format!("pr-{}-{suffix}", pr.number)
    };
    if local.len() > 64 {
        let mut cut = 64;
        while !local.is_char_boundary(cut) {
            cut -= 1;
        }
        local.truncate(cut);
        local = local.trim_end_matches(['-', '.', '/']).to_owned();
    }
    let exists = local_branches.contains(&local);
    CheckoutPlan::Fork { local, exists }
}

/// Makes `name` a valid, option-safe branch name (`git check-ref-format` rules).
#[must_use]
pub fn sanitize_branch(name: &str) -> String {
    let mut s: String = name
        .trim()
        .chars()
        .map(|c| {
            if c.is_alphanumeric() || matches!(c, '-' | '_' | '.' | '/') {
                c
            } else {
                '-'
            }
        })
        .collect();
    while s.contains("..") {
        s = s.replace("..", ".");
    }
    while s.contains("//") {
        s = s.replace("//", "/");
    }
    let s = s
        .split('/')
        .map(|seg| seg.trim_start_matches('.').trim_end_matches(".lock"))
        .filter(|seg| !seg.is_empty())
        .collect::<Vec<_>>()
        .join("/");
    s.trim_start_matches('-')
        .trim_end_matches(['.', '/'])
        .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pr(n: u64, head: &str, fork: bool) -> PullRequest {
        PullRequest {
            number: n,
            head_branch: head.into(),
            from_fork: fork,
            ..PullRequest::default()
        }
    }

    #[test]
    fn same_repo_tracks_the_remote_branch() {
        let plan = checkout_plan(&pr(5, "feature/login", false), "origin", &[]);
        assert_eq!(
            plan,
            CheckoutPlan::Track {
                remote_branch: "origin/feature/login".into(),
                local: "feature/login".into(),
                exists: false
            }
        );
        let plan = checkout_plan(&pr(5, "main", false), "origin", &["main".into()]);
        assert!(matches!(plan, CheckoutPlan::Track { exists: true, .. }));
    }

    #[test]
    fn forks_get_a_prefixed_branch() {
        let plan = checkout_plan(
            &pr(12, "patch-1", true),
            "origin",
            &["pr-12-patch-1".into()],
        );
        assert_eq!(
            plan,
            CheckoutPlan::Fork {
                local: "pr-12-patch-1".into(),
                exists: true
            }
        );
        assert_eq!(checkout_plan(&pr(3, "", true), "o", &[]).local(), "pr-3");
        let long = "x".repeat(100);
        assert!(checkout_plan(&pr(3, &long, true), "o", &[]).local().len() <= 64);
        // An unsafe head name from the same repo is treated like a fork.
        assert_eq!(
            checkout_plan(&pr(9, "-rf ~x", false), "o", &[]).local(),
            "pr-9-rf--x"
        );
    }

    #[test]
    fn sanitizes_branch_names() {
        assert_eq!(sanitize_branch("feat/a b"), "feat/a-b");
        assert_eq!(sanitize_branch("../x..y//z.lock"), "x.y/z");
        assert_eq!(sanitize_branch("-opt"), "opt");
        assert_eq!(sanitize_branch("a:b^c~d?"), "a-b-c-d-");
        assert_eq!(local_head_ref(4), "refs/ferrumgit/pr/4/head");
        assert_eq!(local_base_ref(4), "refs/ferrumgit/pr/4/base");
    }
}

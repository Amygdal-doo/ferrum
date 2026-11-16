//! Interactive rebase through git's own `rebase -i`: the todo list is
//! generated from a [`RebasePlan`] and handed to git as its sequence editor
//! (`cp <todo>`), so git does all the rewriting and stops on conflicts and
//! `edit` lines exactly as it does in a terminal.

use crate::error::{GitError, GitResult};
use crate::parse::FIELD;
use crate::read::{Repo, check_rev};
use crate::runner::Output;
use ferrumgit_domain::{Oid, RebaseAction, RebasePlan, RebaseProgress, RebaseStep};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// More commits than this are not offered for an interactive rebase.
pub const MAX_REBASE_STEPS: usize = 500;

/// Quotes `s` for `sh` (git runs the sequence editor and `exec` lines in a shell).
#[must_use]
pub fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

impl Repo {
    /// Folder for the todo list and reworded messages (inside the git dir,
    /// so it survives until the rebase ends and is never committed).
    fn rebase_scratch(&self) -> PathBuf {
        self.git_dir.join("ferrumgit-rebase")
    }

    /// Removes the scratch folder once no rebase is in progress.
    pub(crate) fn clear_rebase_scratch(&self) {
        let dir = self.rebase_scratch();
        if self.operation().is_none()
            && dir.exists()
            && let Err(error) = std::fs::remove_dir_all(&dir)
        {
            tracing::debug!(%error, "could not remove the rebase scratch folder");
        }
    }

    /// The commits after `base` up to HEAD, oldest first, as `pick` steps.
    /// Refuses ranges with merge commits (`rebase -i` would flatten them).
    pub async fn rebase_steps(&self, base: &str) -> GitResult<Vec<RebaseStep>> {
        check_rev(base)?;
        let ancestor = self
            .run(
                self.cmd(["merge-base", "--is-ancestor", base, "HEAD"])
                    .ok_codes(&[1]),
            )
            .await?;
        if ancestor.code == 1 {
            return Err(GitError::Invalid(
                "that commit is not part of the current branch's history".into(),
            ));
        }
        let range = format!("{base}..HEAD");
        let count = self
            .run(self.cmd(["rev-list", "--count", &range]))
            .await?
            .stdout_str()
            .trim()
            .parse::<usize>()
            .unwrap_or(0);
        if count == 0 {
            return Err(GitError::Invalid(
                "there are no commits after that one on the current branch".into(),
            ));
        }
        if count > MAX_REBASE_STEPS {
            return Err(GitError::Invalid(format!(
                "{count} commits is too many for an interactive rebase here (at most {MAX_REBASE_STEPS})"
            )));
        }
        let out = self
            .run(self.cmd(["log", "--reverse", "-z", "--format=%H%x1f%P%x1f%B", &range]))
            .await?;
        parse_steps(&out.stdout_str())
    }

    /// Starts `rebase -i` with the plan's todo list. Stops (with an error
    /// listing the conflicts) like git does; `edit` lines stop successfully.
    pub(crate) async fn interactive_rebase(
        &self,
        plan: &RebasePlan,
        identity: Option<(&str, &str)>,
    ) -> GitResult<Output> {
        plan.validate().map_err(GitError::Invalid)?;
        check_rev(plan.base.as_str())?;
        if let Some(op) = self.operation() {
            return Err(GitError::Invalid(format!(
                "a {} is already in progress; continue or abort it first",
                op.label().to_lowercase()
            )));
        }
        // The plan must cover exactly the commits git will list, or git
        // would silently drop the ones it does not mention.
        let current: HashSet<Oid> = self
            .rebase_steps(plan.base.as_str())
            .await?
            .into_iter()
            .map(|s| s.oid)
            .collect();
        let planned: HashSet<Oid> = plan.steps.iter().map(|s| s.oid.clone()).collect();
        if current != planned || planned.len() != plan.steps.len() {
            return Err(GitError::Invalid(
                "the branch changed since the rebase was planned; open it again".into(),
            ));
        }

        let dir = self.rebase_scratch();
        let io = |e: std::io::Error| GitError::Spawn(e.to_string());
        if dir.exists() {
            std::fs::remove_dir_all(&dir).map_err(io)?;
        }
        std::fs::create_dir_all(&dir).map_err(io)?;
        let message_file = |i: usize| dir.join(format!("message-{i}.txt"));
        for (i, step) in plan.steps.iter().enumerate() {
            if step.action == RebaseAction::Reword {
                std::fs::write(message_file(i), step.final_message()).map_err(io)?;
            }
        }
        let todo_path = dir.join("todo");
        let todo = plan.todo(|i| shell_quote(&path_str(&message_file(i))));
        std::fs::write(&todo_path, todo).map_err(io)?;

        let editor = format!("cp {}", shell_quote(&path_str(&todo_path)));
        let cmd = self
            .cmd(["rebase", "-i", "--no-autosquash", plan.base.as_str()])
            .env("GIT_SEQUENCE_EDITOR", editor)
            .identity(identity);
        let result = self.run(cmd).await;
        self.clear_rebase_scratch();
        result
    }

    /// Progress of the rebase in progress (merge backend), from its state files.
    #[must_use]
    pub fn rebase_progress(&self) -> Option<RebaseProgress> {
        let dir = self.git_dir.join("rebase-merge");
        if !dir.is_dir() {
            return None;
        }
        let read = |name: &str| std::fs::read_to_string(dir.join(name)).ok();
        let number = |name: &str| read(name).and_then(|s| s.trim().parse::<usize>().ok());
        let lines = |name: &str| {
            read(name)
                .map(|s| {
                    s.lines()
                        .map(str::trim)
                        .filter(|l| !l.is_empty() && !l.starts_with('#'))
                        .map(str::to_owned)
                        .collect::<Vec<_>>()
                })
                .unwrap_or_default()
        };
        let done = lines("done");
        let remaining = lines("git-rebase-todo");
        Some(RebaseProgress {
            step: number("msgnum").unwrap_or(done.len()),
            total: number("end").unwrap_or(done.len() + remaining.len()),
            current: done.last().cloned(),
            editing: dir.join("amend").is_file(),
        })
    }
}

fn path_str(p: &Path) -> String {
    p.to_string_lossy().into_owned()
}

/// `git log -z --format=%H%x1f%P%x1f%B` → steps; merges are refused.
fn parse_steps(text: &str) -> GitResult<Vec<RebaseStep>> {
    let mut steps = Vec::new();
    for record in text.split('\0') {
        let record = record.trim_start_matches('\n');
        if record.is_empty() {
            continue;
        }
        let mut fields = record.splitn(3, FIELD);
        let (Some(oid), Some(parents), Some(message)) =
            (fields.next(), fields.next(), fields.next())
        else {
            return Err(GitError::Parse("rebase commit list".into()));
        };
        if parents.split_whitespace().count() > 1 {
            return Err(GitError::Invalid(format!(
                "{} is a merge commit; interactive rebase of merges is not supported",
                &oid[..oid.len().min(7)]
            )));
        }
        steps.push(RebaseStep::new(
            Oid::new(oid.trim()),
            message.trim_end().to_owned(),
        ));
    }
    Ok(steps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quotes_for_the_shell() {
        assert_eq!(shell_quote("/a b/c"), "'/a b/c'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
    }

    #[test]
    fn parses_steps_and_refuses_merges() {
        let a = "a".repeat(40);
        let b = "b".repeat(40);
        let text = format!("{a}\u{1f}{b}\u{1f}Subject\n\nBody\n\0\n{b}\u{1f}{a}\u{1f}Two\n\0");
        let steps = parse_steps(&text).expect("steps");
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0].message, "Subject\n\nBody");
        assert_eq!(steps[1].summary(), "Two");
        let merge = format!("{a}\u{1f}{b} {a}\u{1f}Merge\0");
        assert!(parse_steps(&merge).is_err());
    }
}

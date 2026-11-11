//! Build, test and run tasks: found from the project's files (Cargo, Go,
//! package.json scripts, Makefile, `.vscode/tasks.json`), run in the
//! integrated terminal, and their output turned into problems.

mod detect;
mod matcher;

use std::path::PathBuf;

pub use detect::detect;
pub use matcher::{BuildProblem, parse_problems, strip_ansi};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum TaskGroup {
    Build,
    Test,
    Run,
    Lint,
    Other,
}

impl TaskGroup {
    pub fn label(self) -> &'static str {
        match self {
            TaskGroup::Build => "build",
            TaskGroup::Test => "test",
            TaskGroup::Run => "run",
            TaskGroup::Lint => "lint",
            TaskGroup::Other => "task",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Task {
    /// What the picker shows, e.g. "cargo build" or "pnpm run dev".
    pub label: String,
    /// Shell command line.
    pub command: String,
    pub cwd: PathBuf,
    pub group: TaskGroup,
    /// Where it came from: "cargo", "go", "npm", "make", "vscode".
    pub source: &'static str,
    /// The task ⌘B runs.
    pub default_build: bool,
}

/// The task ⌘B runs: a task marked default, else the first build task.
pub fn default_build(tasks: &[Task]) -> Option<&Task> {
    tasks
        .iter()
        .find(|t| t.default_build)
        .or_else(|| tasks.iter().find(|t| t.group == TaskGroup::Build))
}

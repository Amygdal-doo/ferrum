//! Run in parallel: the same task given to N agents (2 or 3), each in its
//! own git worktree of the project with its own chat, their diffs side by
//! side on the Parallel page, and **Keep this one**, which applies the
//! chosen worktree's changes to the project's working tree and removes the
//! run's worktrees. The project's own files are never touched before that;
//! with uncommitted changes there, Keep asks to stash them first (they stay
//! in `git stash list`). The git work is in [`worktree`], the page in
//! `page`.

mod page;
pub mod worktree;

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumagent_ui::core::Mode;

use crate::app::{ChatAction, FerrumEngineer};
use worktree::Diff;

/// Agents a run may start.
pub const COUNTS: [usize; 2] = [2, 3];

/// One agent of a run: its worktree and what it changed so far.
pub struct Lane {
    pub path: PathBuf,
    pub diff: Option<Result<Diff, String>>,
}

pub struct Run {
    pub project: PathBuf,
    /// The commit every worktree started from.
    pub base: String,
    pub task: String,
    pub lanes: Vec<Lane>,
}

/// A question the page is waiting on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Confirm {
    /// Keep lane `lane`; `dirty` lists the project's uncommitted changes,
    /// which are stashed first.
    Keep { lane: usize, dirty: Vec<String> },
    /// Throw the whole run away.
    Discard,
    /// Remove a worktree left by an earlier run.
    RemoveLeftover(PathBuf),
}

pub struct ParallelState {
    pub task: Entity<TextInput>,
    pub count: usize,
    pub run: Option<Run>,
    pub confirm: Option<Confirm>,
    /// What is being done right now (creating, keeping…).
    pub busy: Option<String>,
    /// This project's worktrees left by runs that did not finish.
    pub leftovers: Vec<PathBuf>,
    pub(crate) tasks: Vec<Task<()>>,
}

impl ParallelState {
    pub fn new(window: &mut Window, cx: &mut Context<FerrumEngineer>) -> Self {
        let task = cx.new(|cx| {
            TextInput::new(window, cx)
                .multiline(3)
                .borderless()
                .placeholder("The task every agent gets")
        });
        Self {
            task,
            count: 2,
            run: None,
            confirm: None,
            busy: None,
            leftovers: Vec::new(),
            tasks: Vec::new(),
        }
    }

    /// Whether `root` is one of the current run's worktrees.
    pub fn is_lane(&self, root: &Path) -> bool {
        self.lane_index(root).is_some()
    }

    fn lane_index(&self, root: &Path) -> Option<usize> {
        self.run.as_ref()?.lanes.iter().position(|l| l.path == root)
    }
}

/// `~/.ferrum/vibe/worktrees`.
fn worktrees_dir(data_dir: Option<&Path>) -> Option<PathBuf> {
    data_dir.map(|d| d.join("worktrees"))
}

impl FerrumEngineer {
    fn spawn_parallel(&mut self, task: Task<()>) {
        self.parallel.tasks.retain(|t| !t.is_ready());
        self.parallel.tasks.push(task);
    }

    fn parallel_failed(&mut self, error: String, cx: &mut Context<Self>) {
        self.parallel.busy = None;
        self.show_notice(Notice::error(error), cx);
    }

    /// "Run in parallel": makes the worktrees, then starts the task in a
    /// chat in each.
    pub fn start_parallel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let task = self.parallel.task.read(cx).text().trim().to_string();
        let (Some(project), Some(dir)) = (
            self.project.clone(),
            worktrees_dir(self.data_dir.as_deref()),
        ) else {
            self.show_notice(Notice::error("Open a project folder first."), cx);
            return;
        };
        if task.is_empty() || self.parallel.run.is_some() || self.parallel.busy.is_some() {
            return;
        }
        let count = self.parallel.count;
        let prefix = format!(
            "{}-{}",
            ferrum_ui::project::project_name(&project),
            chrono::Local::now().format("%Y%m%d-%H%M%S")
        );
        self.parallel.busy = Some(format!("Creating {count} worktrees…"));
        let root = project.clone();
        let made = cx.background_spawn(async move {
            let base = worktree::base_of(&root)?;
            let paths = worktree::create(&root, &base, &dir, &prefix, count)?;
            Ok::<_, String>((base, paths))
        });
        let task_text = task.clone();
        let job = cx.spawn_in(window, async move |this, cx| {
            let made = made.await;
            this.update_in(cx, |this, window, cx| match made {
                Ok((base, paths)) => {
                    this.parallel.busy = None;
                    this.parallel.run = Some(Run {
                        project,
                        base,
                        task: task_text.clone(),
                        lanes: paths
                            .iter()
                            .map(|path| Lane {
                                path: path.clone(),
                                diff: None,
                            })
                            .collect(),
                    });
                    for path in paths {
                        let start = ChatAction::StartTask(Mode::Agent, Some(task_text.clone()));
                        this.chat_action(&path, start, window, cx);
                    }
                    this.parallel
                        .task
                        .update(cx, |input, cx| input.set_text("", cx));
                    cx.notify();
                }
                Err(error) => this.parallel_failed(error, cx),
            })
            .ok();
        });
        self.spawn_parallel(job);
        cx.notify();
    }

    /// A lane's chat changed files: re-reads its diff.
    pub(crate) fn parallel_changed(&mut self, root: &Path, cx: &mut Context<Self>) {
        if let Some(lane) = self.parallel.lane_index(root) {
            self.refresh_lane(lane, cx);
        }
    }

    pub(crate) fn refresh_lane(&mut self, lane: usize, cx: &mut Context<Self>) {
        let Some(run) = &self.parallel.run else {
            return;
        };
        let Some(path) = run.lanes.get(lane).map(|l| l.path.clone()) else {
            return;
        };
        let base = run.base.clone();
        let diff = cx.background_spawn(async move { worktree::diff(&path, &base) });
        let job = cx.spawn(async move |this, cx| {
            let diff = diff.await;
            this.update(cx, |this, cx| {
                if let Some(lane) = this
                    .parallel
                    .run
                    .as_mut()
                    .and_then(|run| run.lanes.get_mut(lane))
                {
                    lane.diff = Some(diff);
                    cx.notify();
                }
            })
            .ok();
        });
        self.spawn_parallel(job);
    }

    /// "Keep this one": checks the project's working tree first.
    pub fn ask_keep(&mut self, lane: usize, cx: &mut Context<Self>) {
        let Some(run) = &self.parallel.run else {
            return;
        };
        let project = run.project.clone();
        let dirty = cx.background_spawn(async move { worktree::dirty(&project) });
        let job = cx.spawn(async move |this, cx| {
            let dirty = dirty.await;
            this.update(cx, |this, cx| match dirty {
                Ok(dirty) => {
                    this.parallel.confirm = Some(Confirm::Keep { lane, dirty });
                    cx.notify();
                }
                Err(error) => this.parallel_failed(error, cx),
            })
            .ok();
        });
        self.spawn_parallel(job);
    }

    /// The answer to the page's question (`false`: cancel).
    pub fn answer_parallel(&mut self, yes: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(confirm) = self.parallel.confirm.take() else {
            return;
        };
        if yes {
            match confirm {
                Confirm::Keep { lane, dirty } => {
                    self.keep_lane(lane, !dirty.is_empty(), window, cx)
                }
                Confirm::Discard => self.end_run(None, cx),
                Confirm::RemoveLeftover(path) => self.remove_leftover(path, cx),
            }
        }
        cx.notify();
    }

    /// Applies lane `lane`'s changes to the project (stashing its own
    /// first when `stash`), then removes the run's worktrees.
    fn keep_lane(&mut self, lane: usize, stash: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(run) = &self.parallel.run else {
            return;
        };
        let Some(path) = run.lanes.get(lane).map(|l| l.path.clone()) else {
            return;
        };
        let (project, base) = (run.project.clone(), run.base.clone());
        self.parallel.busy = Some(format!("Applying worktree {}…", lane + 1));
        // The agents stop before their files are read.
        self.stop_lanes(cx);
        let message = format!("ferrum-parallel: before keeping worktree {}", lane + 1);
        let kept = cx.background_spawn(async move {
            if worktree::base_of(&project)? != base {
                return Err(
                    "The project's HEAD moved since the run started: commit or check out the \
                     original commit first, or copy the changes from the worktree by hand."
                        .to_string(),
                );
            }
            let diff = worktree::diff(&path, &base)?;
            let stashed = if stash {
                Some(worktree::stash(&project, &message)?)
            } else {
                None
            };
            if let Err(error) = worktree::apply(&project, &diff.patch) {
                if let Some(oid) = &stashed {
                    worktree::unstash(&project, oid)?;
                }
                return Err(format!(
                    "The changes did not apply, nothing was changed: {error}"
                ));
            }
            Ok(stashed.map(|_| message))
        });
        let job = cx.spawn_in(window, async move |this, cx| {
            let kept = kept.await;
            this.update_in(cx, |this, window, cx| match kept {
                Ok(stashed) => {
                    let note = match stashed {
                        Some(message) => format!(
                            "Kept worktree {}. Your earlier changes are stashed as \"{message}\" (git stash list).",
                            lane + 1
                        ),
                        None => format!("Kept worktree {}.", lane + 1),
                    };
                    this.end_run(Some(note), cx);
                    this.refresh_git(window, cx);
                    if let Some(editor) = &this.editor {
                        editor.update(cx, |editor, cx| editor.reload_changed(cx));
                    }
                }
                Err(error) => this.parallel_failed(error, cx),
            })
            .ok();
        });
        self.spawn_parallel(job);
        cx.notify();
    }

    fn stop_lanes(&mut self, cx: &mut Context<Self>) {
        let Some(run) = &self.parallel.run else {
            return;
        };
        for lane in &run.lanes {
            if let Some(slot) = self.chats.get(&lane.path) {
                slot.view.update(cx, |chat, cx| chat.stop_all(cx));
            }
        }
    }

    /// Stops the run's chats and removes its worktrees; `note` is shown.
    fn end_run(&mut self, note: Option<String>, cx: &mut Context<Self>) {
        self.stop_lanes(cx);
        let Some(run) = self.parallel.run.take() else {
            return;
        };
        let paths: Vec<PathBuf> = run.lanes.iter().map(|l| l.path.clone()).collect();
        for path in &paths {
            self.chats.remove(path);
        }
        self.parallel.busy = Some("Removing the worktrees…".into());
        let project = run.project.clone();
        let removed = cx.background_spawn(async move {
            paths
                .iter()
                .filter_map(|path| worktree::remove(&project, path).err())
                .collect::<Vec<_>>()
        });
        let job = cx.spawn(async move |this, cx| {
            let errors = removed.await;
            this.update(cx, |this, cx| {
                this.parallel.busy = None;
                match errors.first() {
                    Some(error) => this.show_notice(Notice::error(error.clone()), cx),
                    None => {
                        if let Some(note) = note {
                            this.show_notice(Notice::info(note), cx);
                        }
                    }
                }
                this.refresh_leftovers(cx);
            })
            .ok();
        });
        self.spawn_parallel(job);
        cx.notify();
    }

    /// Re-reads this project's worktrees left by unfinished runs.
    pub(crate) fn refresh_leftovers(&mut self, cx: &mut Context<Self>) {
        let (Some(project), Some(dir)) = (
            self.project.clone(),
            worktrees_dir(self.data_dir.as_deref()),
        ) else {
            return;
        };
        let lanes: Vec<PathBuf> = self
            .parallel
            .run
            .iter()
            .flat_map(|r| r.lanes.iter().map(|l| l.path.clone()))
            .collect();
        let found = cx.background_spawn(async move { worktree::leftovers(&project, &dir) });
        let job = cx.spawn(async move |this, cx| {
            let mut found = found.await;
            found.retain(|path| !lanes.contains(path));
            this.update(cx, |this, cx| {
                this.parallel.leftovers = found;
                cx.notify();
            })
            .ok();
        });
        self.spawn_parallel(job);
    }

    fn remove_leftover(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone() else {
            return;
        };
        let removed = cx.background_spawn(async move { worktree::remove(&project, &path) });
        let job = cx.spawn(async move |this, cx| {
            let removed = removed.await;
            this.update(cx, |this, cx| {
                if let Err(error) = removed {
                    this.show_notice(Notice::error(error), cx);
                }
                this.refresh_leftovers(cx);
            })
            .ok();
        });
        self.spawn_parallel(job);
    }
}

//! Build, test and run tasks: found in the project (see
//! `ferrumeditor_core::tasks`), run in a terminal tab, and their output's
//! errors and warnings shown as problems.

use ferrum_ui::prelude::*;
use ferrumeditor_core::lsp::Severity;
use ferrumeditor_core::tasks::{self, Task, TaskGroup};

use super::Workspace;
use crate::lsp_store::LspStore;
use crate::picker::{PickerItem, PickerKind};

impl Workspace {
    fn tasks(&self) -> Vec<Task> {
        self.root.as_deref().map(tasks::detect).unwrap_or_default()
    }

    pub(super) fn task_items(&self) -> Vec<PickerItem> {
        self.tasks()
            .into_iter()
            .enumerate()
            .map(|(i, task)| {
                let detail = if task.default_build {
                    format!("{} · ⌘B", task.group.label())
                } else {
                    task.group.label().to_string()
                };
                PickerItem::new(task.label, detail, i.to_string())
            })
            .collect()
    }

    pub(super) fn run_task_payload(
        &mut self,
        payload: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let task = payload
            .parse::<usize>()
            .ok()
            .and_then(|i| self.tasks().into_iter().nth(i));
        if let Some(task) = task {
            self.run_task(task, window, cx);
        }
    }

    /// ⌘B: the project's default build task.
    pub(super) fn run_build_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let all = self.tasks();
        match tasks::default_build(&all) {
            Some(task) => self.run_task(task.clone(), window, cx),
            None if self.root.is_none() => self.info("Open a folder to build it (⌘⇧O)", cx),
            None => self.info(
                "No build task found: add Cargo.toml, go.mod, a package.json \"build\" script, a Makefile or .vscode/tasks.json",
                cx,
            ),
        }
    }

    pub(super) fn run_test_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self
            .tasks()
            .into_iter()
            .find(|t| t.group == TaskGroup::Test)
        {
            Some(task) => self.run_task(task, window, cx),
            None => self.info("No test task found", cx),
        }
    }

    pub(super) fn rerun_last_task(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.last_task.clone() {
            Some(task) => self.run_task(task, window, cx),
            None => self.toggle_picker(PickerKind::Tasks, window, cx),
        }
    }

    /// Saves edited files, then runs `task` in a terminal tab.
    pub(super) fn run_task(&mut self, task: Task, window: &mut Window, cx: &mut Context<Self>) {
        for editor in self.all_editors() {
            let needs_save = {
                let e = editor.read(cx);
                e.path.is_some() && e.is_dirty()
            };
            if needs_save && let Err(error) = editor.update(cx, |e, cx| e.save(None, cx)) {
                self.error(format!("Could not save before running: {error}"), cx);
                return;
            }
        }
        self.run_task_in_terminal(&task.label, &task.command, &task.cwd, window, cx);
        self.info(format!("Running {}", task.label), cx);
        self.last_task = Some(task);
        cx.notify();
    }

    /// A task ended: its problems replace the previous run's, and a notice
    /// sums it up.
    pub(super) fn on_task_finished(
        &mut self,
        label: &str,
        cwd: &std::path::Path,
        code: i32,
        output: &str,
        cx: &mut Context<Self>,
    ) {
        let problems = tasks::parse_problems(output, cwd);
        let errors = problems
            .iter()
            .filter(|p| p.severity == Severity::Error)
            .count();
        let warnings = problems
            .iter()
            .filter(|p| p.severity == Severity::Warning)
            .count();
        let source = label
            .split_whitespace()
            .next()
            .unwrap_or("task")
            .to_string();
        if let Some(store) = LspStore::global(cx) {
            store.update(cx, |store, cx| {
                store.set_build_problems(problems, &source, cx)
            });
        }
        let counts = match (errors, warnings) {
            (0, 0) => String::new(),
            (e, 0) => format!(": {e} error{}", plural(e)),
            (0, w) => format!(": {w} warning{}", plural(w)),
            (e, w) => format!(": {e} error{}, {w} warning{}", plural(e), plural(w)),
        };
        if code == 0 {
            self.info(format!("{label} finished{counts}"), cx);
        } else {
            self.error(
                format!(
                    "{label} failed (exit code {code}){counts}. Ctrl+Shift+M lists the problems."
                ),
                cx,
            );
        }
    }
}

fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

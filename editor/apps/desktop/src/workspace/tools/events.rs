//! What the embedded tools tell the editor: files to open, commands to run
//! in the terminal, changes made on disk, commits — and running commands
//! (tasks, CLI agents, debuggee programs) in the Term pane.

use std::path::{Path, PathBuf};

use ferrum_ui::prelude::*;
use ferrumagent_ui::{ChatEvent, ChatView};
use ferrumeditor_core::agents::shell_line;
use ferrumeditor_core::tool_layout::ToolKind;
use ferrumgit_ui::{GitView, GitViewEvent};
use ferrumterm_ui::{CommandId, TermView, TermViewEvent};

use super::super::Workspace;

/// A task running in the terminal: its output is copied to `output` so
/// its errors and warnings become problems when it ends.
pub(in crate::workspace) struct RunningTask {
    pub label: String,
    pub cwd: PathBuf,
    pub output: Option<PathBuf>,
}

impl Workspace {
    pub(super) fn on_git_view_event(
        &mut self,
        _: &Entity<GitView>,
        event: &GitViewEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            GitViewEvent::OpenFile { path, line } => self.open_tool_file(path, *line, window, cx),
            GitViewEvent::RepoChanged { .. } => self.git_changed(cx),
            GitViewEvent::CommitCreated { .. } => {
                self.git_changed(cx);
                for editor in self.all_editors() {
                    editor.update(cx, |e, cx| e.load_git_base(cx));
                }
            }
        }
    }

    pub(super) fn on_chat_event(
        &mut self,
        _: &Entity<ChatView>,
        event: &ChatEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChatEvent::OpenFile { path, line } => self.open_tool_file(path, *line, window, cx),
            ChatEvent::RunInTerminal { cmd, cwd } => {
                self.run_in_terminal(cmd, Some(cwd.clone()), window, cx);
            }
            // The agent wrote files: open buffers reload (or ask if edited).
            ChatEvent::ChangesUpdated { .. } => self.check_disk_changes(window, cx),
        }
    }

    pub(super) fn on_term_event(
        &mut self,
        _: &Entity<TermView>,
        event: &TermViewEvent,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let TermViewEvent::CommandFinished {
            command: Some(command),
            exit_code,
            ..
        } = event
        {
            self.command_finished(*command, *exit_code, cx);
        }
    }

    /// Opens `path` (absolute) at a 1-based line in the editor area.
    pub(in crate::workspace) fn open_tool_file(
        &mut self,
        path: &Path,
        line: Option<u32>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let line = line.map(|l| l.saturating_sub(1) as usize);
        self.open_file_at(path, line, None, window, cx);
    }

    fn git_changed(&mut self, cx: &mut Context<Self>) {
        self.project_bar
            .update(cx, |bar, cx| bar.refresh_branch(cx));
        cx.notify();
    }

    /// Runs `command_line` in a new tab of the Term pane (opened, without
    /// taking focus), in `cwd` or the project folder.
    pub fn run_in_terminal(
        &mut self,
        command_line: &str,
        cwd: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> CommandId {
        self.show_tool(ToolKind::Term, window, cx);
        let cwd = cwd.or_else(|| self.root.clone());
        if let Some(log) = self.tools.dry_run.as_mut() {
            log.push(command_line.to_string());
            self.tools.next_dry_id += 1;
            return CommandId(self.tools.next_dry_id);
        }
        let term = self.ensure_term(window, cx);
        term.update(cx, |t, cx| t.run_command(command_line, cwd, cx))
    }

    /// Runs a build / test / run task; its output is kept for problems.
    pub(in crate::workspace) fn run_task_in_terminal(
        &mut self,
        label: &str,
        command: &str,
        cwd: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let output = cfg!(unix).then(|| task_output_path(&self.tools.tasks));
        let line = match &output {
            Some(output) if self.tools.dry_run.is_none() => captured(label, command, output),
            _ => command.to_string(),
        };
        let id = self.run_in_terminal(&line, Some(cwd.to_path_buf()), window, cx);
        self.tools.tasks.insert(
            id,
            RunningTask {
                label: label.to_string(),
                cwd: cwd.to_path_buf(),
                output,
            },
        );
    }

    /// A terminal command ended; a task's problems are collected.
    pub fn command_finished(&mut self, command: CommandId, exit_code: i32, cx: &mut Context<Self>) {
        let Some(task) = self.tools.tasks.remove(&command) else {
            return;
        };
        let output = task
            .output
            .as_ref()
            .and_then(|path| {
                let text = std::fs::read_to_string(path).ok();
                std::fs::remove_file(path).ok();
                text
            })
            .unwrap_or_default();
        self.on_task_finished(&task.label, &task.cwd, exit_code, &output, cx);
    }

    /// Ends the newest running task as if its command printed `output`
    /// and exited with `exit_code`.
    #[cfg(test)]
    pub fn finish_last_task(&mut self, output: &str, exit_code: i32, cx: &mut Context<Self>) {
        let Some(id) = self.tools.tasks.keys().max().copied() else {
            panic!("no task is running");
        };
        if let Some(path) = &self.tools.tasks[&id].output {
            std::fs::write(path, output).expect("task output");
        }
        self.command_finished(id, exit_code, cx);
    }
}

/// A fresh temp file for a task's output.
fn task_output_path(running: &std::collections::HashMap<CommandId, RunningTask>) -> PathBuf {
    let stamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos());
    std::env::temp_dir().join(format!(
        "ferrumeditor-task-{}-{stamp}-{}.log",
        std::process::id(),
        running.len()
    ))
}

/// `command` run by `/bin/sh` with its output shown and copied to
/// `output`; the tab is titled `label` and the exit code is kept.
fn captured(label: &str, command: &str, output: &Path) -> String {
    let file = output.to_string_lossy().to_string();
    let code = format!("{file}.code");
    let script = format!(
        "printf '\\033]2;%s\\007' {title}\n{{ ( {command}\n); echo $? > {code_q}; }} 2>&1 | tee {file_q}\nstatus=$(cat {code_q} 2>/dev/null || echo 1)\nrm -f {code_q}\nexit $status",
        title = shell_line(&[format!("▶ {label}")]),
        code_q = shell_line(&[code]),
        file_q = shell_line(&[file]),
    );
    shell_line(&["/bin/sh".to_string(), "-c".to_string(), script])
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    #[test]
    fn captured_tasks_keep_output_and_exit_code() {
        let dir = tempfile::tempdir().expect("tempdir");
        let out = dir.path().join("task.log");
        let line = super::captured("t", "echo hello; echo oops >&2; exit 3", &out);
        let status = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg(&line)
            .output()
            .expect("run");
        assert_eq!(status.status.code(), Some(3));
        let text = std::fs::read_to_string(&out).expect("output");
        assert!(text.contains("hello") && text.contains("oops"), "{text}");
        assert!(!dir.path().join("task.log.code").exists());
    }
}

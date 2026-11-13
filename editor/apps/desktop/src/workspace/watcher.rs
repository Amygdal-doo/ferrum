//! File watching: the project folder (tree refresh, reload changed files,
//! git branch) and `~/.ferrum/editor/` plus its `themes/` and
//! `icon-themes/` folders (settings and themes applied live). Events
//! from `notify` are collected on a channel and drained on a timer.

use std::path::PathBuf;
use std::sync::mpsc::{Receiver, channel};
use std::time::Duration;

use ferrum_ui::prelude::*;
use notify::{RecursiveMode, Watcher as _};

use super::Workspace;
use crate::settings::Settings;

const POLL: Duration = Duration::from_millis(300);

pub struct Watcher {
    _watcher: notify::RecommendedWatcher,
    _task: Task<()>,
}

impl Workspace {
    pub(super) fn start_watcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.watcher = None;
        let (sender, receiver) = channel::<notify::Result<notify::Event>>();
        let mut watcher = match notify::recommended_watcher(sender) {
            Ok(watcher) => watcher,
            Err(error) => {
                eprintln!("ferrumeditor: file watching unavailable: {error}");
                return;
            }
        };
        if let Some(root) = &self.root
            && let Err(error) = watcher.watch(root, RecursiveMode::Recursive)
        {
            eprintln!("ferrumeditor: cannot watch {}: {error}", root.display());
        }
        if let Some(files) = cx.global::<Settings>().files.clone() {
            let dirs = [
                (&files.dir, RecursiveMode::NonRecursive),
                (&files.themes_dir, RecursiveMode::NonRecursive),
                (&files.icon_themes_dir, RecursiveMode::Recursive),
            ];
            for (dir, mode) in dirs {
                if dir.exists()
                    && let Err(error) = watcher.watch(dir, mode)
                {
                    eprintln!("ferrumeditor: cannot watch {}: {error}", dir.display());
                }
            }
        }
        let task = cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(POLL).await;
                let paths = drain(&receiver);
                if paths.is_empty() {
                    continue;
                }
                if this
                    .update_in(cx, |this, window, cx| this.on_fs_changes(paths, window, cx))
                    .is_err()
                {
                    break;
                }
            }
        });
        self.watcher = Some(Watcher {
            _watcher: watcher,
            _task: task,
        });
    }

    fn on_fs_changes(&mut self, paths: Vec<PathBuf>, window: &mut Window, cx: &mut Context<Self>) {
        let settings_changed = cx.global::<Settings>().files.as_ref().is_some_and(|files| {
            paths
                .iter()
                .any(|p| files.is_settings_file(p) || files.is_theme_path(p))
        });
        if settings_changed {
            self.reload_settings(cx);
        }
        let in_git = |p: &PathBuf| p.components().any(|c| c.as_os_str() == ".git");
        // Build output churns constantly; don't refresh the tree for it.
        let noisy = |p: &PathBuf| {
            p.components()
                .any(|c| matches!(c.as_os_str().to_str(), Some("target" | "node_modules")))
        };
        if paths.iter().any(in_git) {
            self.project_bar
                .update(cx, |bar, cx| bar.refresh_branch(cx));
            for editor in self.all_editors() {
                editor.update(cx, |e, cx| e.load_git_base(cx));
            }
        }
        if let Some(root) = self.root.clone()
            && paths
                .iter()
                .any(|p| p.starts_with(&root) && !in_git(p) && !noisy(p))
        {
            self.file_list = None;
            if let Some(tree) = self.tree.clone() {
                tree.update(cx, |tree, cx| tree.refresh(cx));
            }
        }
        let open_changed = self
            .all_editors()
            .iter()
            .any(|e| e.read(cx).path.as_ref().is_some_and(|p| paths.contains(p)));
        if open_changed {
            self.check_disk_changes(window, cx);
        }
        cx.notify();
    }

    /// Re-reads every settings file and applies it to the running app.
    pub fn reload_settings(&mut self, cx: &mut Context<Self>) {
        let next = cx.global::<Settings>().reload();
        let mut problems = next.problems.clone();
        problems.extend(crate::actions::apply_keymap(&next.keymap, cx));
        let theme = next.theme.clone();
        cx.set_global(next);
        crate::theme_apply::apply(&theme, cx);
        // A default editor theme follows the suite's choice again.
        ferrum_ui::appearance::reapply(cx);
        crate::actions::set_menus(cx);
        for editor in self.all_editors() {
            editor.update(cx, |e, cx| e.apply_settings(cx));
        }
        match problems.first() {
            Some(first) => {
                let more = problems.len() - 1;
                let suffix = if more > 0 {
                    format!(" (+{more} more)")
                } else {
                    String::new()
                };
                self.error(format!("{first}{suffix}"), cx);
            }
            None => self.info("Settings reloaded", cx),
        }
    }
}

fn drain(receiver: &Receiver<notify::Result<notify::Event>>) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for event in receiver.try_iter().flatten() {
        if matches!(event.kind, notify::EventKind::Access(_)) {
            continue;
        }
        for path in event.paths {
            if !paths.contains(&path) {
                paths.push(path);
            }
        }
    }
    paths
}

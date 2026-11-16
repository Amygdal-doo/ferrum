//! Keeping a folder's changes current: a file watcher (or, without one,
//! a timer) flags changes to the files a snapshot reads, and a tick loop
//! rescans in the background when flagged.

use super::scan::{self, Limits, Snapshot, TooBig};
use super::{Baseline, POLL_TICKS, TICK, diff, home};
use crate::app::GitView;
use gpui::{AppContext as _, Context};
use std::sync::Arc;
use std::sync::atomic::Ordering;

impl GitView {
    /// Watches the folder (a file watcher, else a timer) and rescans it
    /// when something it reads changes.
    pub(super) fn watch_folder(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        let (root, dirty) = (folder.path.clone(), folder.dirty.clone());
        let home = home();
        let watcher = notify::recommended_watcher(move |event: notify::Result<notify::Event>| {
            let relevant = match event {
                Ok(event) if matches!(event.kind, notify::EventKind::Access(_)) => false,
                Ok(event) => {
                    event.need_rescan()
                        || event
                            .paths
                            .iter()
                            .any(|p| scan::is_watched(&root, p, home.as_deref()))
                }
                Err(_) => true,
            };
            if relevant {
                dirty.store(true, Ordering::Relaxed);
            }
        })
        .and_then(|mut watcher| {
            use notify::Watcher as _;
            watcher.watch(&folder.path, notify::RecursiveMode::Recursive)?;
            Ok(watcher)
        });
        match watcher {
            Ok(watcher) => folder._watcher = Some(watcher),
            Err(error) => {
                tracing::debug!(error = %error, "no file watcher; polling the folder");
                folder.polling = true;
            }
        }
        let generation = folder.generation;
        folder._watch = Some(cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(TICK).await;
                let go = this.update(cx, |app, cx| app.folder_tick(generation, cx));
                if !matches!(go, Ok(true)) {
                    return;
                }
            }
        }));
    }

    /// One tick of the watch loop: rescans when the folder changed. False
    /// once the baseline it was started for is gone.
    pub(super) fn folder_tick(&mut self, generation: u64, cx: &mut Context<Self>) -> bool {
        let Some(folder) = self.folder.as_mut().filter(|f| f.generation == generation) else {
            return false;
        };
        folder.ticks = folder.ticks.wrapping_add(1);
        if folder.polling && folder.ticks.is_multiple_of(POLL_TICKS) {
            folder.dirty.store(true, Ordering::Relaxed);
        }
        if !folder.scanning && folder.dirty.swap(false, Ordering::Relaxed) {
            self.rescan_folder(cx);
        }
        true
    }

    fn rescan_folder(&mut self, cx: &mut Context<Self>) {
        let Some(folder) = self.folder.as_mut() else {
            return;
        };
        let Some(current) = folder.current.clone() else {
            return;
        };
        folder.scanning = true;
        let (generation, path) = (folder.generation, folder.path.clone());
        // Some room to grow past the baseline before giving up.
        let base = Limits::default();
        let limits = Limits {
            max_files: base.max_files * 2,
            max_bytes: base.max_bytes * 2,
            ..base
        };
        let task = cx.background_spawn(async move {
            scan::snapshot(&path, limits, home().as_deref(), Some(&current))
        });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |app, cx| app.rescanned(generation, result, cx))
                .ok();
        })
        .detach();
    }

    fn rescanned(
        &mut self,
        generation: u64,
        result: Result<Snapshot, TooBig>,
        cx: &mut Context<Self>,
    ) {
        let Some(folder) = self.folder.as_mut().filter(|f| f.generation == generation) else {
            return;
        };
        folder.scanning = false;
        match result {
            Ok(snapshot) => {
                if let Some(base) = &folder.base {
                    folder.changes = diff::changes(base, &snapshot);
                }
                folder.current = Some(Arc::new(snapshot));
                let selected = folder.selected.clone();
                match selected.filter(|s| folder.changes.iter().any(|c| &c.path == s)) {
                    Some(path) => self.select_folder_file(path, cx),
                    None => {
                        folder.selected = None;
                        folder.diff = None;
                    }
                }
            }
            Err(too_big) => {
                folder.baseline = Baseline::TooBig(too_big);
                folder._watcher = None;
                folder._watch = None;
            }
        }
        cx.notify();
    }
}

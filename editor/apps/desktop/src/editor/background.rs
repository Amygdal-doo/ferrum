//! Background work for an editor: tree-sitter highlighting and git line
//! marks, both debounced and computed off the main thread on a snapshot.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use ferrumeditor_core::{git, highlight};
use gpui::Context;

use super::{EditorView, HIGHLIGHT_LIMIT};

const HIGHLIGHT_DEBOUNCE: Duration = Duration::from_millis(20);
const GIT_DEBOUNCE: Duration = Duration::from_millis(250);

impl EditorView {
    pub(crate) fn schedule_highlight(&mut self, cx: &mut Context<Self>) {
        let version = self.state.buffer().version();
        if self.highlight_version == Some(version) {
            return;
        }
        let language = self.state.language;
        if !highlight::supports(language) || self.state.buffer().len() > HIGHLIGHT_LIMIT {
            self.highlights = Arc::default();
            self.highlight_version = Some(version);
            self.highlight_task = None;
            return;
        }
        self.highlight_cancel.store(1, Ordering::Relaxed);
        let cancel = Arc::new(AtomicUsize::new(0));
        self.highlight_cancel = cancel.clone();
        let text = self.state.text();
        let first = self.highlight_version.is_none();
        self.highlight_task = Some(cx.spawn(async move |this, cx| {
            if !first {
                cx.background_executor().timer(HIGHLIGHT_DEBOUNCE).await;
            }
            let flag = cancel.clone();
            let spans = cx
                .background_executor()
                .spawn(async move { highlight::highlight(language, &text, Some(&flag)) })
                .await;
            if cancel.load(Ordering::Relaxed) != 0 {
                return;
            }
            this.update(cx, |this, cx| {
                this.highlights = Arc::new(spans);
                this.highlight_version = Some(version);
                cx.notify();
            })
            .ok();
        }));
    }

    /// Reads the file's HEAD version (if it is tracked) and recomputes marks.
    pub(crate) fn load_git_base(&mut self, cx: &mut Context<Self>) {
        let (Some(repo), Some(path)) = (self.git_repo.clone(), self.path.clone()) else {
            self.git_base = None;
            self.git_marks = Arc::default();
            return;
        };
        self.git_task = Some(cx.spawn(async move |this, cx| {
            let base = cx
                .background_executor()
                .spawn(async move { git::head_contents(&repo, &path) })
                .await;
            this.update(cx, |this, cx| {
                this.git_base = base.map(Arc::new);
                this.git_version = None;
                this.schedule_git_diff(cx);
            })
            .ok();
        }));
    }

    pub(crate) fn schedule_git_diff(&mut self, cx: &mut Context<Self>) {
        let Some(base) = self.git_base.clone() else {
            if !self.git_marks.is_empty() {
                self.git_marks = Arc::default();
                cx.notify();
            }
            return;
        };
        let version = self.state.buffer().version();
        if self.git_version == Some(version) {
            return;
        }
        let text = self.state.text();
        let immediate = self.git_version.is_none();
        self.git_task = Some(cx.spawn(async move |this, cx| {
            if !immediate {
                cx.background_executor().timer(GIT_DEBOUNCE).await;
            }
            let marks = cx
                .background_executor()
                .spawn(async move { git::diff_marks(&base, &text) })
                .await;
            this.update(cx, |this, cx| {
                this.git_marks = Arc::new(marks);
                this.git_version = Some(version);
                cx.notify();
            })
            .ok();
        }));
    }
}

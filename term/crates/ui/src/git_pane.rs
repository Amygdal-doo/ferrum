//! FerrumGIT beside the terminal (the desktop window only), as in
//! FerrumVibe: the pane header's FerrumGIT and Changes toggles show the
//! repository of the focused pane's folder, on its history or on the
//! uncommitted changes and their diffs. It follows the focused pane when
//! it changes directory; files open in Ferrum Editor.

use std::path::PathBuf;

use ferrum_ui::components::{Clickable, IconName, Toggleable};
use ferrumgit_ui::{GitView, GitViewEvent, GitViewOptions};
use gpui::{Context, Entity, IntoElement, ParentElement, Styled, Subscription, Window, div, px};

use crate::view::TermView;

/// Which toggle opened the pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum GitPage {
    History,
    Changes,
}

pub(crate) struct GitPane {
    view: Entity<GitView>,
    /// `None` while hidden (the view is kept, with its state).
    shown: Option<GitPage>,
    /// Repository folder last given to the view.
    repo: Option<PathBuf>,
    _events: Subscription,
}

/// Width of the pane: a share of the window, within FerrumGIT's minimum.
const SHARE: f32 = 0.42;

impl TermView {
    /// The repository folder the pane follows: the focused pane's project,
    /// when it is inside a git work tree.
    fn git_folder(&self) -> Option<PathBuf> {
        self.project
            .as_ref()
            .and_then(|p| p.root())
            .filter(|dir| in_work_tree(dir))
    }

    /// Shows the pane on `page`, or hides it when it already shows `page`.
    pub(crate) fn toggle_git(
        &mut self,
        page: GitPage,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.git.as_ref().and_then(|g| g.shown) == Some(page) {
            if let Some(git) = self.git.as_mut() {
                git.shown = None;
            }
            cx.notify();
            return;
        }
        let Some(folder) = self.git_folder() else {
            self.app
                .notify("This tab's folder is not a git repository: cd into one to see it here.");
            cx.notify();
            return;
        };
        if self.git.is_none() {
            let options = GitViewOptions::embedded();
            let view = GitView::new(None, options, window, cx);
            let events = cx.subscribe(&view, |this, _, event: &GitViewEvent, cx| {
                if let GitViewEvent::OpenFile { path, line } = event {
                    let project = this.git_folder();
                    if let Err(error) =
                        ferrum_ui::project::open_file_in_editor(project.as_deref(), path, *line)
                    {
                        this.app.notify(error);
                        cx.notify();
                    }
                }
            });
            self.git = Some(GitPane {
                view,
                shown: None,
                repo: None,
                _events: events,
            });
        }
        let Some(git) = self.git.as_mut() else {
            return;
        };
        git.shown = Some(page);
        git.view.update(cx, |view, cx| {
            if git.repo.as_ref() != Some(&folder) {
                view.set_repo(&folder, cx);
            }
            match page {
                GitPage::History => view.show_history(cx),
                GitPage::Changes => view.show_changes(cx),
            }
        });
        git.repo = Some(folder);
        cx.notify();
    }

    /// Follows the focused pane's project while the pane is shown.
    pub(crate) fn sync_git(&mut self, cx: &mut Context<Self>) {
        let folder = self.git_folder();
        let Some(git) = self.git.as_mut().filter(|g| g.shown.is_some()) else {
            return;
        };
        if let Some(folder) = folder.filter(|f| git.repo.as_ref() != Some(f)) {
            git.view.update(cx, |view, cx| view.set_repo(&folder, cx));
            git.repo = Some(folder);
        }
    }

    /// The FerrumGIT and Changes toggles for the pane header.
    pub(crate) fn git_toggles(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let shown = self.git.as_ref().and_then(|g| g.shown);
        let toggle = |id: &'static str, icon, tip: &'static str, page: GitPage| {
            ferrum_ui::title_button(id, icon, tip)
                .toggle_state(shown == Some(page))
                .on_click(cx.listener(move |view, _, window, cx| view.toggle_git(page, window, cx)))
        };
        gpui::div()
            .flex()
            .flex_none()
            .gap_0p5()
            .child(toggle(
                "toggle-git",
                IconName::GitBranch,
                "FerrumGIT: history and branches",
                GitPage::History,
            ))
            .child(toggle(
                "toggle-changes",
                IconName::Code,
                "Code changes: uncommitted files and their diffs",
                GitPage::Changes,
            ))
    }

    /// The pane, when shown: a card to the right of the terminal.
    pub(crate) fn git_card(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let git = self.git.as_ref().filter(|g| g.shown.is_some())?;
        Some(
            ferrum_ui::pane_card(false, cx)
                .h_full()
                .flex_basis(gpui::relative(SHARE))
                .flex_shrink(1.)
                .min_w(px(ferrumgit_ui::MIN_WIDTH))
                .overflow_hidden()
                .child(div().size_full().child(git.view.clone())),
        )
    }
}

/// Whether `dir` or a folder above it has `.git` (a folder, or a file for
/// worktrees and submodules).
fn in_work_tree(dir: &std::path::Path) -> bool {
    dir.ancestors().any(|d| d.join(".git").exists())
}

#[cfg(test)]
mod tests {
    use super::in_work_tree;

    #[test]
    fn finds_the_work_tree_above_a_folder() {
        let dir = tempfile::tempdir().expect("tempdir");
        let nested = dir.path().join("a/b");
        std::fs::create_dir_all(&nested).expect("mkdir");
        assert!(!in_work_tree(&nested));
        std::fs::create_dir(dir.path().join("a/.git")).expect("git dir");
        assert!(in_work_tree(&nested));
    }
}

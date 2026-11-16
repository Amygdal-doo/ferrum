//! Files and folders dragged in from Finder or the desktop. Files open
//! like any other (text in an editor tab, images in the image viewer,
//! other binaries in the binary-file view), in the pane they were dropped
//! on; a folder becomes the project of a window without one, is revealed
//! in the tree when it is inside the project, and otherwise opens in the
//! window of its project or a new window.

use ferrum_ui::prelude::*;
use ferrumeditor_core::cli::OpenTarget;
use gpui::{ExternalPaths, StyleRefinement};

use super::Workspace;

/// How a pane (or the window) looks while files are dragged over it.
pub(super) fn drop_highlight(style: StyleRefinement, cx: &App) -> StyleRefinement {
    let accent = cx.theme().colors().text_accent;
    style
        .bg(accent.opacity(0.08))
        .border_2()
        .border_color(accent)
}

impl Workspace {
    /// Opens `paths` dropped on pane `pane` (by index), or on the window
    /// outside the panes (`None`: the active pane).
    pub(super) fn open_dropped(
        &mut self,
        paths: &ExternalPaths,
        pane: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(pane) = pane.filter(|p| *p < self.panes.len()) {
            self.active_pane = pane;
        }
        let mut elsewhere = Vec::new();
        for path in paths.paths() {
            if !path.is_dir() {
                self.open_file(path, window, cx);
                continue;
            }
            let folder = path.canonicalize().unwrap_or_else(|_| path.clone());
            match self.root.clone() {
                None => self.set_root(folder, window, cx),
                Some(root) if folder.starts_with(&root) => {
                    if let Some(tree) = self.tree.clone() {
                        tree.update(cx, |tree, cx| tree.reveal(&folder, cx));
                    }
                    self.tree_visible = true;
                }
                Some(_) => elsewhere.push(OpenTarget {
                    path: folder,
                    line: None,
                    column: None,
                }),
            }
        }
        // Other projects: their window, or a new one (after this window's
        // update, which routing may need to touch).
        for target in elsewhere {
            cx.defer(move |cx| crate::windows::route(vec![target], cx));
        }
        cx.notify();
    }
}

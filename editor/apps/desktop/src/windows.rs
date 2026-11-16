//! Several windows in one process, one project each. [`open_window`]
//! builds a window around a new [`Workspace`]; [`route`] sends paths that
//! arrive while the app runs (another `ferrumeditor` call, Finder's Open
//! With, a drop on the Dock icon) to the window of their project, or to a
//! new window.

use std::path::{Path, PathBuf};

use ferrumeditor_core::cli::OpenTarget;
use gpui::{AnyWindowHandle, App, AppContext as _, Entity, Global, WeakEntity, Window};

use crate::workspace::Workspace;

/// Every open workspace window, in the order they were opened.
#[derive(Default)]
struct WorkspaceWindows(Vec<(AnyWindowHandle, WeakEntity<Workspace>)>);

impl Global for WorkspaceWindows {}

/// The open workspaces with their windows (closed ones dropped).
pub fn all(cx: &mut App) -> Vec<(AnyWindowHandle, Entity<Workspace>)> {
    let open = cx.windows();
    let windows = &mut cx.default_global::<WorkspaceWindows>().0;
    windows.retain(|(handle, workspace)| {
        open.iter().any(|h| h.window_id() == handle.window_id()) && workspace.upgrade().is_some()
    });
    windows
        .iter()
        .filter_map(|(handle, workspace)| Some((*handle, workspace.upgrade()?)))
        .collect()
}

/// Builds the workspace of a new window: opens `targets` (or restores the
/// last session when `restore` and there are none), reports `problems`,
/// and asks before the window closes with unsaved changes.
pub fn build_workspace(
    targets: Vec<OpenTarget>,
    restore: bool,
    problems: &[String],
    window: &mut Window,
    cx: &mut App,
) -> Entity<Workspace> {
    let workspace = cx.new(|cx| {
        let mut workspace = Workspace::new(window, cx);
        if targets.is_empty() {
            if restore {
                workspace.restore_session(window, cx);
            }
        } else {
            workspace.open_targets(targets, window, cx);
        }
        if let Some(first) = problems.first() {
            workspace.error(
                format!("{first} (+{} more)", problems.len() - 1).replace(" (+0 more)", ""),
                cx,
            );
        }
        workspace
    });
    let weak = workspace.downgrade();
    window.on_window_should_close(cx, move |window, cx| {
        // The last window quits (asking about unsaved changes and saving
        // the session); others close on their own.
        weak.update(cx, |workspace, cx| workspace.close_window(window, cx))
            .is_err()
    });
    cx.default_global::<WorkspaceWindows>()
        .0
        .push((window.window_handle(), workspace.downgrade()));
    workspace
}

/// Opens a new window with `targets`.
pub fn open_window(targets: Vec<OpenTarget>, cx: &mut App) {
    let build =
        move |window: &mut Window, cx: &mut App| build_workspace(targets, false, &[], window, cx);
    // Headless tests have no main window style (and no onboarding).
    let opened = if cfg!(test) {
        cx.open_window(gpui::WindowOptions::default(), build)
            .map(|_| ())
    } else {
        ferrum_ui::open_app_window(cx, build).map(|_| ())
    };
    if let Err(error) = opened {
        tracing::error!("could not open a window: {error:#}");
    }
}

/// Where a request's paths go.
#[derive(Debug, PartialEq, Eq)]
pub enum Route {
    /// The window at this index of [`all`] (front-most first, see [`route`]).
    Existing(usize, Vec<OpenTarget>),
    New(Vec<OpenTarget>),
}

/// Splits `targets` between open windows (`roots`, front-most first;
/// `None` = a window without a folder). A folder argument picks the window
/// whose project contains it (with all the files), else a new window; a
/// lone file goes to the window whose project contains it, else to a
/// window without a folder, else a new one.
pub fn plan(targets: Vec<OpenTarget>, roots: &[Option<PathBuf>]) -> Vec<Route> {
    let project_of = |path: &Path| {
        roots
            .iter()
            .enumerate()
            .filter_map(|(i, root)| Some((i, root.as_deref()?)))
            .filter(|(_, root)| path.starts_with(root))
            .max_by_key(|(_, root)| root.as_os_str().len())
            .map(|(i, _)| i)
    };
    if let Some(folder) = targets.iter().find(|t| t.path.is_dir()) {
        return vec![match project_of(&folder.path) {
            Some(index) => Route::Existing(
                index,
                targets.into_iter().filter(|t| !t.path.is_dir()).collect(),
            ),
            None => Route::New(targets),
        }];
    }
    let mut routes: Vec<Route> = Vec::new();
    let mut add = |route_index: Option<usize>, target: OpenTarget| {
        for route in &mut routes {
            match (route, route_index) {
                (Route::Existing(i, list), Some(j)) if *i == j => {
                    list.push(target);
                    return;
                }
                (Route::New(list), None) => {
                    list.push(target);
                    return;
                }
                _ => {}
            }
        }
        routes.push(match route_index {
            Some(i) => Route::Existing(i, vec![target]),
            None => Route::New(vec![target]),
        });
    };
    let loose = roots.iter().position(Option::is_none);
    for target in targets {
        add(project_of(&target.path).or(loose), target);
    }
    routes
}

/// Opens `targets` in the right windows and brings them forward. No
/// targets: brings the front-most window forward.
pub fn route(targets: Vec<OpenTarget>, cx: &mut App) {
    let mut windows = all(cx);
    // Front-most first, where the platform knows the order.
    if let Some(stack) = cx.window_stack() {
        windows.sort_by_key(|(handle, _)| {
            stack
                .iter()
                .position(|h| h.window_id() == handle.window_id())
                .unwrap_or(usize::MAX)
        });
    }
    cx.activate(true);
    if targets.is_empty() {
        match windows.first() {
            Some((handle, _)) => {
                handle
                    .update(cx, |_, window, _| window.activate_window())
                    .ok();
            }
            None => open_window(Vec::new(), cx),
        }
        return;
    }
    // Project roots are canonical; so are the paths compared with them.
    let targets: Vec<OpenTarget> = targets
        .into_iter()
        .map(|t| OpenTarget {
            path: t.path.canonicalize().unwrap_or(t.path),
            ..t
        })
        .collect();
    let roots: Vec<Option<PathBuf>> = windows
        .iter()
        .map(|(_, workspace)| workspace.read(cx).root.clone())
        .collect();
    for route in plan(targets, &roots) {
        match route {
            Route::Existing(index, targets) => {
                let (handle, workspace) = windows[index].clone();
                handle
                    .update(cx, |_, window, cx| {
                        window.activate_window();
                        workspace.update(cx, |w, cx| w.open_targets(targets, window, cx));
                    })
                    .ok();
            }
            Route::New(targets) => open_window(targets, cx),
        }
    }
}

/// `file://` URLs from the platform (Finder, the Dock) as open targets.
pub fn targets_from_urls(urls: &[String]) -> Vec<OpenTarget> {
    urls.iter()
        .filter_map(|url| ferrumeditor_core::lsp::uri_to_path(url))
        .map(|path| OpenTarget {
            path,
            line: None,
            column: None,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn target(path: &Path) -> OpenTarget {
        OpenTarget {
            path: path.to_path_buf(),
            line: None,
            column: None,
        }
    }

    #[test]
    fn plans_by_project() {
        let dir = tempfile::tempdir().expect("tempdir");
        let a = dir.path().join("a");
        let b = dir.path().join("b");
        std::fs::create_dir_all(a.join("src")).expect("mkdir");
        std::fs::create_dir_all(&b).expect("mkdir");
        let roots = vec![Some(a.clone()), None];
        // A folder of an open project goes to its window, with the files.
        let file = a.join("src/main.rs");
        assert_eq!(
            plan(vec![target(&a), target(&file)], &roots),
            vec![Route::Existing(0, vec![target(&file)])]
        );
        // A folder no window has: a new window with everything.
        assert_eq!(
            plan(vec![target(&b)], &roots),
            vec![Route::New(vec![target(&b)])]
        );
        // Lone files: their project's window, else the window without a
        // folder; with none of those, a new window.
        let loose = dir.path().join("notes.md");
        assert_eq!(
            plan(vec![target(&file), target(&loose)], &roots),
            vec![
                Route::Existing(0, vec![target(&file)]),
                Route::Existing(1, vec![target(&loose)]),
            ]
        );
        assert_eq!(
            plan(vec![target(&loose)], &[Some(a.clone())]),
            vec![Route::New(vec![target(&loose)])]
        );
        // The innermost project wins.
        let nested = vec![Some(dir.path().to_path_buf()), Some(a.clone())];
        assert_eq!(
            plan(vec![target(&file)], &nested),
            vec![Route::Existing(1, vec![target(&file)])]
        );
    }
}

//! Several windows in one process: paths handed over by a second launch
//! open in the window of their project or in a new window; closing one of
//! several windows leaves the others.

use std::path::{Path, PathBuf};

use ferrumeditor_core::cli::OpenTarget;
use gpui::TestAppContext;

use crate::tests::init;
use crate::windows;

fn target(path: &Path) -> OpenTarget {
    OpenTarget {
        path: path.to_path_buf(),
        line: None,
        column: None,
    }
}

fn roots(cx: &mut TestAppContext) -> Vec<Option<PathBuf>> {
    cx.update(|cx| {
        windows::all(cx)
            .into_iter()
            .map(|(_, w)| w.read(cx).root.clone())
            .collect()
    })
}

#[gpui::test]
fn launches_open_in_the_project_window_or_a_new_one(cx: &mut TestAppContext) {
    init(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let (a, b) = (root.join("a"), root.join("b"));
    std::fs::create_dir_all(&a).expect("mkdir");
    std::fs::create_dir_all(&b).expect("mkdir");
    std::fs::write(a.join("one.rs"), "fn one() {}\n").expect("write");
    std::fs::write(b.join("two.rs"), "fn two() {}\n").expect("write");
    let first = a.clone();
    cx.update(|cx| {
        cx.open_window(gpui::WindowOptions::default(), |window, cx| {
            windows::build_workspace(vec![target(&first)], false, &[], window, cx)
        })
        .expect("window");
    });
    cx.run_until_parked();
    assert_eq!(roots(cx), vec![Some(a.clone())]);

    // `ferrumeditor a/one.rs:1`: the window of project a.
    cx.update(|cx| {
        windows::route(
            vec![OpenTarget {
                line: Some(1),
                ..target(&a.join("one.rs"))
            }],
            cx,
        )
    });
    cx.run_until_parked();
    assert_eq!(roots(cx).len(), 1);
    let open = cx.update(|cx| {
        let (_, workspace) = windows::all(cx).remove(0);
        workspace
            .read(cx)
            .active_editor()
            .and_then(|e| e.read(cx).path.clone())
    });
    assert_eq!(open, Some(a.join("one.rs")));

    // `ferrumeditor b b/two.rs`: a new window for project b.
    cx.update(|cx| windows::route(vec![target(&b), target(&b.join("two.rs"))], cx));
    cx.run_until_parked();
    assert_eq!(roots(cx), vec![Some(a.clone()), Some(b.clone())]);
    // `ferrumeditor` alone only brings a window forward.
    cx.update(|cx| windows::route(Vec::new(), cx));
    cx.run_until_parked();
    assert_eq!(roots(cx).len(), 2);

    // Closing b's window (nothing unsaved) leaves a's.
    let (handle, workspace) = cx.update(|cx| windows::all(cx).remove(1));
    cx.update(|cx| {
        handle
            .update(cx, |_, window, cx| {
                workspace.update(cx, |w, cx| w.close_window(window, cx))
            })
            .expect("window")
    });
    cx.run_until_parked();
    assert_eq!(roots(cx), vec![Some(a)]);
}

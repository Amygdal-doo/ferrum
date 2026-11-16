//! Files and folders dragged in from Finder: a text file opens in an
//! editor tab, an image in the image viewer, other binaries in the
//! binary-file view, a folder becomes the project, and a file dropped on
//! the right of two panes opens there.

use std::path::{Path, PathBuf};

use gpui::{
    Entity, ExternalPaths, FileDropEvent, Pixels, Point, TestAppContext, VisualTestContext, point,
};

use crate::file_viewer::ViewerContent;
use crate::tests::workspace_window;
use crate::workspace::Workspace;

fn drop_paths(paths: &[&Path], at: Point<Pixels>, cx: &mut VisualTestContext) {
    let paths: Vec<PathBuf> = paths.iter().map(|p| p.to_path_buf()).collect();
    cx.simulate_event(FileDropEvent::Entered {
        position: at,
        paths: ExternalPaths(paths.into()),
    });
    cx.simulate_event(FileDropEvent::Pending { position: at });
    cx.simulate_event(FileDropEvent::Submit { position: at });
    cx.run_until_parked();
}

/// A point at `x` (0..1 of the width) half way down the window.
fn at(x: f32, cx: &mut VisualTestContext) -> Point<Pixels> {
    let size = cx.update(|window, _| window.viewport_size());
    point(size.width * x, size.height * 0.5)
}

fn active_path(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Option<PathBuf> {
    workspace.read_with(cx, |w, cx| {
        w.active_editor()
            .and_then(|e| e.read(cx).path.clone())
            .or_else(|| w.active_viewer().map(|v| v.read(cx).path.clone()))
    })
}

#[gpui::test]
fn dropped_files_and_folders_open(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    let project = root.join("project");
    std::fs::create_dir_all(&project).expect("mkdir");
    let notes = root.join("notes.txt");
    std::fs::write(&notes, "hello\n").expect("write");
    let png = root.join("pixel.png");
    image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
        .save_with_format(&png, image::ImageFormat::Png)
        .expect("png");
    let blob = root.join("data.bin");
    std::fs::write(&blob, [0u8, 159, 146, 150, 0, 1, 2]).expect("write");

    // A text file: an editor tab.
    drop_paths(&[&notes], at(0.5, cx), cx);
    assert_eq!(active_path(&workspace, cx), Some(notes.clone()));
    assert!(workspace.read_with(cx, |w, _| w.active_editor().is_some()));

    // An image and a binary file together: viewer tabs, the last active.
    drop_paths(&[&png, &blob], at(0.5, cx), cx);
    assert_eq!(active_path(&workspace, cx), Some(blob.clone()));
    let viewer = workspace
        .read_with(cx, |w, _| w.active_viewer())
        .expect("viewer");
    // Not an image: the binary-file view.
    assert!(!viewer.read_with(cx, |v, _| matches!(v.content, ViewerContent::Image { .. })));
    assert_eq!(workspace.read_with(cx, |w, _| w.active_pane_tab_count()), 3);
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("pane:previous-tab", window, cx)
    });
    let image = workspace
        .read_with(cx, |w, _| w.active_viewer())
        .expect("image viewer");
    assert!(image.read_with(cx, |v, _| matches!(v.content, ViewerContent::Image { .. })));

    // A folder: the project of a window without one.
    drop_paths(&[&project], at(0.5, cx), cx);
    assert_eq!(
        workspace.read_with(cx, |w, _| w.root.clone()),
        Some(project.clone())
    );

    // Two panes: a file dropped on the right one opens there.
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("pane:split-right", window, cx)
    });
    workspace.update_in(cx, |w, window, cx| {
        w.run_command("pane:focus-previous", window, cx)
    });
    cx.run_until_parked();
    assert_eq!(workspace.read_with(cx, |w, _| w.pane_position()), (0, 2));
    let other = project.join("other.rs");
    std::fs::write(&other, "fn other() {}\n").expect("write");
    drop_paths(&[&other], at(0.9, cx), cx);
    assert_eq!(workspace.read_with(cx, |w, _| w.pane_position()), (1, 2));
    assert_eq!(active_path(&workspace, cx), Some(other));
}

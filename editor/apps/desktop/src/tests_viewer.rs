//! Images and other binary files open in a viewer tab instead of failing
//! as "binary file".

use std::path::{Path, PathBuf};

use ferrumeditor_core::images::ImageFormat;
use gpui::{Entity, TestAppContext, VisualTestContext};

use crate::file_viewer::{FileViewer, FileViewerEvent, ImageInfo, ViewerContent};
use crate::tests::{primary, workspace_window};
use crate::workspace::Workspace;

const SVG: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" width="10" height="5"><rect width="10" height="5" fill="red"/></svg>"#;

/// A `width` × `height` image file encoded as `format`.
fn write_image(path: &Path, format: image::ImageFormat, width: u32, height: u32) {
    let image = image::RgbImage::from_pixel(width, height, image::Rgb([30, 120, 200]));
    image.save_with_format(path, format).expect("write image");
}

fn canonical_dir() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().canonicalize().expect("canonical");
    (dir, root)
}

fn open(workspace: &Entity<Workspace>, path: &Path, cx: &mut VisualTestContext) {
    workspace.update_in(cx, |w, window, cx| {
        w.open_file(path, window, cx);
    });
    cx.run_until_parked();
}

fn draw(cx: &mut VisualTestContext) {
    cx.run_until_parked();
    cx.update(|window, cx| window.draw(cx).clear(cx));
}

fn viewer(workspace: &Entity<Workspace>, cx: &mut VisualTestContext) -> Entity<FileViewer> {
    workspace
        .read_with(cx, |w, _| w.active_viewer())
        .expect("a viewer tab is active")
}

fn image_info(viewer: &Entity<FileViewer>, cx: &mut VisualTestContext) -> Option<ImageInfo> {
    viewer.read_with(cx, |v, _| match &v.content {
        ViewerContent::Image { info, .. } => Some(*info),
        _ => None,
    })
}

#[gpui::test]
fn png_and_jpg_open_in_the_image_viewer(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let (_dir, root) = canonical_dir();
    let png = root.join("icon.png");
    let jpg = root.join("photo.JPG");
    write_image(&png, image::ImageFormat::Png, 4, 3);
    write_image(&jpg, image::ImageFormat::Jpeg, 8, 6);

    open(&workspace, &png, cx);
    let png_viewer = viewer(&workspace, cx);
    assert_eq!(
        image_info(&png_viewer, cx),
        Some(ImageInfo {
            format: ImageFormat::Png,
            width: 4,
            height: 3
        })
    );
    png_viewer.read_with(cx, |v, _| {
        assert_eq!(v.title().as_ref(), "icon.png");
        assert!(v.file_size.is_some_and(|size| size > 0));
    });
    assert!(workspace.read_with(cx, |w, _| w.active_editor().is_none()));
    assert_eq!(workspace.read_with(cx, |w, _| w.notice_message()), None);

    open(&workspace, &jpg, cx);
    let jpg_viewer = viewer(&workspace, cx);
    assert_eq!(
        image_info(&jpg_viewer, cx),
        Some(ImageInfo {
            format: ImageFormat::Jpeg,
            width: 8,
            height: 6
        })
    );
    assert_eq!(workspace.read_with(cx, |w, _| w.notice_message()), None);
    draw(cx);
    assert!(cx.debug_bounds("viewer-footer").is_some());

    // The tree marks the image as the active file.
    workspace.update_in(cx, |w, window, cx| w.set_root(root.clone(), window, cx));
    let tree = workspace.read_with(cx, |w, _| w.tree()).expect("tree");
    tree.read_with(cx, |t, _| {
        assert_eq!(t.active_file(), Some(jpg.as_path()));
    });
}

#[gpui::test]
fn reopening_an_image_activates_its_tab(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let (_dir, root) = canonical_dir();
    let png = root.join("logo.png");
    let text = root.join("notes.txt");
    write_image(&png, image::ImageFormat::Png, 2, 2);
    std::fs::write(&text, "hello\n").expect("write");

    open(&workspace, &png, cx);
    let first = viewer(&workspace, cx);
    open(&workspace, &text, cx);
    assert!(workspace.read_with(cx, |w, _| w.active_viewer().is_none()));
    open(&workspace, &png, cx);
    assert_eq!(viewer(&workspace, cx), first);
    assert_eq!(workspace.read_with(cx, |w, _| w.active_pane_tab_count()), 2);

    // Closed, it comes back with ⇧⌘T.
    cx.simulate_keystrokes(&primary("cmd-w"));
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |w, _| w.active_viewer().is_none()));
    workspace.update_in(cx, |w, window, cx| w.reopen_closed_tab(window, cx));
    cx.run_until_parked();
    assert_eq!(
        viewer(&workspace, cx).read_with(cx, |v, _| v.path.clone()),
        png
    );
}

#[gpui::test]
fn a_damaged_image_says_why(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let (_dir, root) = canonical_dir();
    let bad = root.join("broken.png");
    std::fs::write(&bad, b"\x89PNG\r\n\x1a\n not really").expect("write");

    open(&workspace, &bad, cx);
    let viewer = viewer(&workspace, cx);
    let message = viewer.read_with(cx, |v, _| match &v.content {
        ViewerContent::Failed(message) => message.to_string(),
        _ => panic!("expected a failure message"),
    });
    assert!(message.contains("PNG"), "{message}");
    assert_eq!(workspace.read_with(cx, |w, _| w.notice_message()), None);
    draw(cx);
    assert!(cx.debug_bounds("viewer-message").is_some());
}

#[gpui::test]
fn svgs_open_as_images_and_as_text(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let (_dir, root) = canonical_dir();
    let svg = root.join("mark.svg");
    std::fs::write(&svg, SVG).expect("write");

    open(&workspace, &svg, cx);
    let viewer = viewer(&workspace, cx);
    assert_eq!(
        image_info(&viewer, cx),
        Some(ImageInfo {
            format: ImageFormat::Svg,
            width: 10,
            height: 5
        })
    );

    // "Open as Text" opens the source in an editor tab beside it.
    viewer.update(cx, |_, cx| {
        cx.emit(FileViewerEvent::OpenAsText(svg.clone()))
    });
    cx.run_until_parked();
    let editor = workspace
        .read_with(cx, |w, _| w.active_editor())
        .expect("editor");
    editor.read_with(cx, |e, _| assert_eq!(e.state.text(), SVG));
    assert_eq!(workspace.read_with(cx, |w, _| w.active_pane_tab_count()), 2);
}

#[gpui::test]
fn other_binaries_offer_other_apps(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let (_dir, root) = canonical_dir();
    let pdf = root.join("manual.pdf");
    std::fs::write(&pdf, b"%PDF-1.7\n\x00\x01\x02binary").expect("write");

    open(&workspace, &pdf, cx);
    let viewer = viewer(&workspace, cx);
    viewer.read_with(cx, |v, _| {
        assert!(matches!(v.content, ViewerContent::Binary));
        assert!(!v.is_image());
    });
    assert_eq!(workspace.read_with(cx, |w, _| w.notice_message()), None);
    draw(cx);
    assert!(cx.debug_bounds("viewer-message").is_some());
}

#[gpui::test]
fn zoom_keys_and_reloading_from_disk(cx: &mut TestAppContext) {
    let (workspace, cx) = workspace_window(cx);
    let (_dir, root) = canonical_dir();
    let png = root.join("big.png");
    write_image(&png, image::ImageFormat::Png, 40, 20);
    open(&workspace, &png, cx);
    let viewer = viewer(&workspace, cx);
    draw(cx);

    // ⌘= / ⌘- step the zoom; ⌘0 fits again. The editor font is untouched.
    let font = |cx: &mut VisualTestContext| {
        cx.read(|cx| cx.global::<crate::settings::Settings>().font_size_delta)
    };
    let before = font(cx);
    let fitted = viewer.read_with(cx, |v, _| v.scale());
    cx.simulate_keystrokes(&primary("cmd-="));
    let zoomed = viewer.read_with(cx, |v, _| v.zoom).expect("zoomed");
    assert!(zoomed > fitted, "{zoomed} > {fitted}");
    cx.simulate_keystrokes(&primary("cmd--"));
    assert!(viewer.read_with(cx, |v, _| v.zoom).expect("zoomed") < zoomed);
    cx.simulate_keystrokes(&primary("cmd-0"));
    assert_eq!(viewer.read_with(cx, |v, _| v.zoom), None);
    assert_eq!(font(cx), before);

    // A change on disk re-reads the image.
    write_image(&png, image::ImageFormat::Png, 50, 25);
    workspace.update_in(cx, |w, window, cx| {
        w.on_fs_changes(vec![png.clone()], window, cx)
    });
    cx.run_until_parked();
    assert_eq!(
        image_info(&viewer, cx).map(|i| (i.width, i.height)),
        Some((50, 25))
    );
}

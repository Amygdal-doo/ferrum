use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

fn all_shown() -> Layout {
    let mut layout = Layout::default();
    layout.set_visible(Column::Git, true);
    layout.set_visible(Column::Term, true);
    layout
}

#[test]
fn default_shows_only_the_chat() {
    let layout = Layout::default();
    assert_eq!(layout.order(), Column::ALL);
    assert_eq!(layout.visible(), [Column::Chat]);
    assert_eq!(layout.fractions(), [(Column::Chat, 1.0)]);
    let total: f32 = all_shown().fractions().iter().map(|(_, f)| f).sum();
    assert!(close(total, 1.0));
}

#[test]
fn panes_toggle_but_the_chat_stays() {
    let mut layout = all_shown();
    assert!(!layout.set_visible(Column::Chat, false));
    assert!(layout.is_visible(Column::Chat));
    assert!(layout.set_visible(Column::Git, false));
    assert!(!layout.set_visible(Column::Git, false));
    assert_eq!(layout.visible(), [Column::Chat, Column::Term]);
    let fractions = layout.fractions();
    assert!(close(fractions[0].1 + fractions[1].1, 1.0));
    // Hidden panes keep their width.
    layout.set_visible(Column::Git, true);
    assert_eq!(layout, all_shown());
}

#[test]
fn resizing_moves_one_border_within_limits() {
    let mut layout = all_shown();
    // Border between Chat and Git to 60%.
    assert!(layout.resize(Column::Chat, 0.6, 0.1));
    let f = layout.fractions();
    assert!(close(f[0].1, 0.6), "{f:?}");
    assert!(close(f[1].1, 0.15), "{f:?}");
    assert!(close(f[2].1, 0.25), "{f:?}");
    // Clamped so Git keeps its minimum.
    layout.resize(Column::Chat, 0.99, 0.1);
    let f = layout.fractions();
    assert!(close(f[1].1, 0.1), "{f:?}");
    // The last pane has no border on its right.
    assert!(!layout.resize(Column::Term, 0.5, 0.1));
    // With Git hidden, Chat's border is with Term.
    layout.set_visible(Column::Git, false);
    assert!(layout.resize(Column::Chat, 0.25, 0.1));
    let f = layout.fractions();
    assert!(close(f[0].1, 0.25) && close(f[1].1, 0.75), "{f:?}");
}

#[test]
fn bad_layouts_are_repaired() {
    let json = r#"{"slots":[
        {"column":"term","visible":true,"weight":-1},
        {"column":"term","visible":false,"weight":1},
        {"column":"chat","visible":false,"weight":0.5}]}"#;
    let layout = serde_json::from_str::<Layout>(json)
        .expect("parse")
        .normalized();
    // The chat comes first and is shown; the missing Git pane is added.
    assert_eq!(layout.order(), [Column::Chat, Column::Term, Column::Git]);
    assert_eq!(layout.visible(), [Column::Chat, Column::Term, Column::Git]);
    assert!(layout.slots.iter().all(|s| s.weight > 0.0));
}

#[test]
fn layouts_are_saved_per_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("vibe").join("layouts.json");
    let (a, b) = (Path::new("/code/a"), Path::new("/code/b"));
    assert_eq!(load(&file, Some(a)), Layout::default());

    let mut with_git = Layout::default();
    with_git.set_visible(Column::Git, true);
    save(&file, Some(a), &with_git).expect("save a");
    let mut with_term = Layout::default();
    with_term.set_visible(Column::Term, true);
    with_term.resize(Column::Chat, 0.7, 0.1);
    save(&file, Some(b), &with_term).expect("save b");

    assert_eq!(load(&file, Some(a)), with_git);
    assert_eq!(load(&file, Some(b)), with_term);
    // A project without its own layout starts from the last one used.
    assert_eq!(load(&file, Some(Path::new("/code/new"))), with_term);
    assert_eq!(load(&file, None), with_term);
    std::fs::write(&file, "not json").expect("write");
    assert_eq!(load(&file, Some(a)), Layout::default());
}

#[test]
fn the_files_pane_is_shown_by_default_and_its_width_is_kept_in_limits() {
    let mut layout = Layout::default();
    assert!(layout.files.visible);
    assert!(close(layout.files.width, DEFAULT_FILES_WIDTH));
    assert!(layout.files.resize(10.));
    assert!(close(layout.files.width, MIN_FILES_WIDTH));
    assert!(!layout.files.resize(MIN_FILES_WIDTH));
    assert!(layout.files.resize(5000.));
    assert!(close(layout.files.width, MAX_FILES_WIDTH));
    // Layouts saved before the Files pane, or with a broken width.
    let old: Layout =
        serde_json::from_str(r#"{"slots":[{"column":"chat","visible":true,"weight":1.0}]}"#)
            .expect("parse");
    assert_eq!(old.normalized().files, FilesPane::default());
    let broken: Layout =
        serde_json::from_str(r#"{"slots":[],"files":{"visible":false,"width":1e9}}"#)
            .expect("parse");
    let broken = broken.normalized();
    assert!(!broken.files.visible);
    assert!(close(broken.files.width, MAX_FILES_WIDTH));
}

#[test]
fn the_files_pane_round_trips_per_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("layouts.json");
    let layout = Layout {
        files: FilesPane {
            visible: false,
            width: 333.,
        },
        ..Layout::default()
    };
    save(&file, Some(Path::new("/p")), &layout).expect("save");
    assert_eq!(load(&file, Some(Path::new("/p"))).files, layout.files);
}

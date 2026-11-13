use super::*;

fn close(a: f32, b: f32) -> bool {
    (a - b).abs() < 1e-3
}

#[test]
fn default_shows_three_columns_in_order() {
    let layout = Layout::default();
    assert_eq!(layout.visible(), Column::ALL);
    let total: f32 = layout.fractions().iter().map(|(_, f)| f).sum();
    assert!(close(total, 1.0));
}

#[test]
fn hidden_columns_keep_their_width() {
    let mut layout = Layout::default();
    assert!(layout.set_visible(Column::Git, false));
    assert!(!layout.set_visible(Column::Git, false));
    assert_eq!(layout.visible(), [Column::Chat, Column::Term]);
    let fractions = layout.fractions();
    assert!(close(fractions[0].1 + fractions[1].1, 1.0));
    layout.set_visible(Column::Git, true);
    assert_eq!(layout, Layout::default());
    // Every column can be closed.
    for column in Column::ALL {
        layout.set_visible(column, false);
    }
    assert!(layout.visible().is_empty());
    assert!(layout.fractions().is_empty());
}

#[test]
fn moves_left_right_and_by_drop() {
    let mut layout = Layout::default();
    assert!(!layout.move_to_side(Column::Chat, Side::Left));
    assert!(layout.move_to_side(Column::Chat, Side::Right));
    assert_eq!(layout.order(), [Column::Git, Column::Chat, Column::Term]);
    // Hidden columns are skipped: Term moves past the hidden Chat.
    layout.set_visible(Column::Chat, false);
    assert_eq!(
        layout.neighbour(Column::Term, Side::Left),
        Some(Column::Git)
    );
    assert!(layout.move_to_side(Column::Term, Side::Left));
    assert_eq!(layout.order(), [Column::Term, Column::Git, Column::Chat]);
    layout.set_visible(Column::Chat, true);
    // Dropping a header on another column takes that column's place.
    assert!(layout.move_onto(Column::Term, Column::Chat));
    assert_eq!(layout.order(), [Column::Git, Column::Chat, Column::Term]);
    assert!(layout.move_onto(Column::Term, Column::Git));
    assert_eq!(layout.order(), [Column::Term, Column::Git, Column::Chat]);
    assert!(!layout.move_onto(Column::Git, Column::Git));
}

#[test]
fn resizing_moves_one_border_within_limits() {
    let mut layout = Layout::default();
    // Border between Chat and Git to 50%.
    assert!(layout.resize(Column::Chat, 0.5, 0.1));
    let f = layout.fractions();
    assert!(close(f[0].1, 0.5), "{f:?}");
    assert!(close(f[1].1, 0.18), "{f:?}");
    assert!(close(f[2].1, 0.32), "{f:?}");
    // Clamped so Git keeps its minimum.
    layout.resize(Column::Chat, 0.99, 0.1);
    let f = layout.fractions();
    assert!(close(f[1].1, 0.1), "{f:?}");
    // The last column has no border on its right.
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
    assert_eq!(layout.order(), [Column::Term, Column::Chat, Column::Git]);
    assert_eq!(layout.visible(), [Column::Term, Column::Git]);
    assert!(layout.slots.iter().all(|s| s.weight > 0.0));
}

#[test]
fn layouts_are_saved_per_project() {
    let dir = tempfile::tempdir().expect("tempdir");
    let file = dir.path().join("agentic").join("layouts.json");
    let (a, b) = (Path::new("/code/a"), Path::new("/code/b"));
    assert_eq!(load(&file, Some(a)), Layout::default());

    let mut only_chat = Layout::default();
    only_chat.set_visible(Column::Git, false);
    only_chat.set_visible(Column::Term, false);
    save(&file, Some(a), &only_chat).expect("save a");
    let mut reordered = Layout::default();
    reordered.move_onto(Column::Term, Column::Chat);
    save(&file, Some(b), &reordered).expect("save b");

    assert_eq!(load(&file, Some(a)), only_chat);
    assert_eq!(load(&file, Some(b)), reordered);
    // A project without its own layout starts from the last one used.
    assert_eq!(load(&file, Some(Path::new("/code/new"))), reordered);
    assert_eq!(load(&file, None), reordered);
    std::fs::write(&file, "not json").expect("write");
    assert_eq!(load(&file, Some(a)), Layout::default());
}

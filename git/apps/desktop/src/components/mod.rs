pub mod actions;
pub mod code;
pub mod detail;
pub mod dialogs;
pub mod diff;
pub mod file_panel;
pub mod graph;
pub mod highlight;
pub mod home;
pub mod icons;
pub mod menu;
pub mod shell;
pub mod sidebar;
pub mod split;
pub mod widgets;

use ferrumgit_domain::ChangeKind;
use gpui::{App, Hsla, rgb};
use theme::ActiveTheme;

/// Lane colours (readable on the dark theme).
pub const LANE_COLORS: [u32; 8] = [
    0x3b82f6, 0x22c55e, 0xf59e0b, 0xef4444, 0xa855f7, 0x06b6d4, 0xec4899, 0x84cc16,
];

#[must_use]
pub fn lane_color(i: usize) -> Hsla {
    rgb(LANE_COLORS[i % LANE_COLORS.len()]).into()
}

/// Colour of a change letter in file lists.
#[must_use]
pub fn change_color(k: ChangeKind, cx: &App) -> Hsla {
    let status = cx.theme().status();
    match k {
        ChangeKind::Added | ChangeKind::Untracked => status.created,
        ChangeKind::Deleted => status.deleted,
        ChangeKind::Renamed | ChangeKind::Copied => status.renamed,
        ChangeKind::Conflicted => status.conflict,
        ChangeKind::Modified | ChangeKind::TypeChanged | ChangeKind::Unmodified => status.modified,
    }
}

/// `added` / `modified` / … and its colour, as in the details file list.
#[must_use]
pub fn change_word(k: ChangeKind, cx: &App) -> (&'static str, Hsla) {
    let word = match k {
        ChangeKind::Added | ChangeKind::Untracked => "added",
        ChangeKind::Deleted => "deleted",
        ChangeKind::Renamed => "renamed",
        ChangeKind::Copied => "copied",
        ChangeKind::Conflicted => "conflict",
        ChangeKind::Modified | ChangeKind::TypeChanged | ChangeKind::Unmodified => "modified",
    };
    (word, change_color(k, cx))
}

/// `dir/sub/file.rs` → (`file.rs`, `dir/sub/`).
#[must_use]
pub fn split_path(path: &str) -> (&str, &str) {
    match path.rfind('/') {
        Some(i) => (&path[i + 1..], &path[..=i]),
        None => (path, ""),
    }
}

/// Current Unix time in seconds.
#[must_use]
pub fn now_secs() -> i64 {
    chrono::Utc::now().timestamp()
}

/// Avatar background derived from the author's email (stable per person).
#[must_use]
pub fn avatar_color(seed: &str) -> Hsla {
    const PALETTE: [u32; 6] = [0x64748b, 0x0ea5e9, 0x10b981, 0xf59e0b, 0x8b5cf6, 0xf43f5e];
    let h = seed
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    let color: Hsla = rgb(PALETTE[h as usize % PALETTE.len()]).into();
    color.opacity(0.35)
}

#[must_use]
pub fn plural(n: usize) -> &'static str {
    if n == 1 { "" } else { "s" }
}

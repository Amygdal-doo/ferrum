pub mod actions;
pub mod code;
pub mod detail;
pub mod dialogs;
pub mod diff;
pub mod graph;
pub mod highlight;
pub mod home;
pub mod icons;
pub mod menu;
pub mod rust_ui;
pub mod shell;
pub mod sidebar;

use ferrumgit_domain::ChangeKind;

/// Lane colours (used as SVG strokes; readable on light and dark backgrounds).
pub const LANE_COLORS: [&str; 8] = [
    "#3b82f6", "#22c55e", "#f59e0b", "#ef4444", "#a855f7", "#06b6d4", "#ec4899", "#84cc16",
];

#[must_use]
pub fn lane_color(i: usize) -> &'static str {
    LANE_COLORS[i % LANE_COLORS.len()]
}

/// Colour class for a change letter. Literal strings so Tailwind keeps them.
#[must_use]
pub const fn change_class(k: ChangeKind) -> &'static str {
    match k {
        ChangeKind::Added | ChangeKind::Untracked => "text-green-600 dark:text-green-400",
        ChangeKind::Deleted => "text-red-600 dark:text-red-400",
        ChangeKind::Renamed | ChangeKind::Copied => "text-blue-600 dark:text-blue-400",
        ChangeKind::Conflicted => "text-orange-600 dark:text-orange-400",
        ChangeKind::Modified | ChangeKind::TypeChanged | ChangeKind::Unmodified => {
            "text-amber-600 dark:text-amber-400"
        }
    }
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
pub fn avatar_class(seed: &str) -> &'static str {
    const PALETTE: [&str; 6] = [
        "bg-slate-500/30 text-foreground",
        "bg-sky-500/30 text-foreground",
        "bg-emerald-500/30 text-foreground",
        "bg-amber-500/30 text-foreground",
        "bg-violet-500/30 text-foreground",
        "bg-rose-500/30 text-foreground",
    ];
    let h = seed
        .bytes()
        .fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(u32::from(b)));
    PALETTE[h as usize % PALETTE.len()]
}

/// `added` / `modified` / … and its colour, as in the details file list.
#[must_use]
pub const fn change_word(k: ChangeKind) -> (&'static str, &'static str, &'static str) {
    match k {
        ChangeKind::Added | ChangeKind::Untracked => (
            "added",
            "text-green-600 dark:text-green-400",
            "bg-green-600 text-white",
        ),
        ChangeKind::Deleted => (
            "deleted",
            "text-red-600 dark:text-red-400",
            "bg-red-600 text-white",
        ),
        ChangeKind::Renamed => (
            "renamed",
            "text-violet-600 dark:text-violet-400",
            "bg-violet-600 text-white",
        ),
        ChangeKind::Copied => (
            "copied",
            "text-violet-600 dark:text-violet-400",
            "bg-violet-600 text-white",
        ),
        ChangeKind::Conflicted => (
            "conflict",
            "text-orange-600 dark:text-orange-400",
            "bg-orange-600 text-white",
        ),
        ChangeKind::Modified | ChangeKind::TypeChanged | ChangeKind::Unmodified => (
            "modified",
            "text-blue-600 dark:text-blue-400",
            "bg-blue-600 text-white",
        ),
    }
}

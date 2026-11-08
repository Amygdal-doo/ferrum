//! Stroke icons (24×24, Lucide-style paths drawn by hand) as inline SVG.

use dioxus::prelude::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Icon {
    Terminal,
    Folder,
    FolderOpen,
    Refresh,
    Pull,
    Push,
    Fetch,
    Sync,
    Merge,
    Rebase,
    Branch,
    Tag,
    Cloud,
    Archive,
    ArchiveRestore,
    Search,
    ChevronLeft,
    ChevronRight,
    ChevronDown,
    QuickOpen,
    History,
    Layers,
    Sliders,
    User,
    Bookmark,
    Plus,
    Check,
}

fn paths(icon: Icon) -> &'static [&'static str] {
    match icon {
        Icon::Terminal => &["M4 17l6-6-6-6", "M12 19h8"],
        Icon::Folder => &[
            "M4 20h16a2 2 0 0 0 2-2V8a2 2 0 0 0-2-2h-7.9a2 2 0 0 1-1.7-.9l-.8-1.2A2 2 0 0 0 7.9 3H4a2 2 0 0 0-2 2v13a2 2 0 0 0 2 2z",
        ],
        Icon::FolderOpen => &[
            "M6 14l1.5-2.9A2 2 0 0 1 9.2 10H20a2 2 0 0 1 1.9 2.5l-1.5 6A2 2 0 0 1 18.4 20H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2h3.9a2 2 0 0 1 1.7.9l.8 1.2a2 2 0 0 0 1.7.9H18a2 2 0 0 1 2 2v2",
        ],
        Icon::Refresh => &[
            "M21 12a9 9 0 1 1-9-9c2.5 0 4.9 1 6.7 2.7L21 8",
            "M21 3v5h-5",
        ],
        Icon::Pull => &["M12 17V3", "M6 11l6 6 6-6", "M19 21H5"],
        Icon::Push => &["M18 9l-6-6-6 6", "M12 3v14", "M5 21h14"],
        Icon::Fetch => &[
            "M12 13v8",
            "M8 17l4 4 4-4",
            "M4.4 15.3A7 7 0 1 1 15.7 8h1.8a4.5 4.5 0 0 1 2.4 8.3",
        ],
        Icon::Sync => &["M21 16l-4 4-4-4", "M17 20V4", "M3 8l4-4 4 4", "M7 4v16"],
        Icon::Merge => &[
            "M6 21V9a9 9 0 0 0 9 9",
            "M18 15a3 3 0 1 1 0 6 3 3 0 0 1 0-6z",
            "M6 3a3 3 0 1 1 0 6 3 3 0 0 1 0-6z",
        ],
        Icon::Rebase | Icon::Branch => &[
            "M6 3v12",
            "M18 9a9 9 0 0 1-9 9",
            "M18 3a3 3 0 1 1 0 6 3 3 0 0 1 0-6z",
            "M6 15a3 3 0 1 1 0 6 3 3 0 0 1 0-6z",
        ],
        Icon::Tag => &[
            "M12.6 2.6A2 2 0 0 0 11.2 2H4a2 2 0 0 0-2 2v7.2a2 2 0 0 0 .6 1.4l8.7 8.7a2.4 2.4 0 0 0 3.4 0l6.6-6.6a2.4 2.4 0 0 0 0-3.4z",
            "M7.5 7.5h.01",
        ],
        Icon::Cloud => &["M17.5 19H9a7 7 0 1 1 6.7-9h1.8a4.5 4.5 0 1 1 0 9z"],
        Icon::Archive => &[
            "M3 3h18v5H3z",
            "M4 8v11a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8",
            "M10 12h4",
        ],
        Icon::ArchiveRestore => &[
            "M3 3h18v5H3z",
            "M4 8v11a2 2 0 0 0 2 2h2",
            "M20 8v11a2 2 0 0 1-2 2h-2",
            "M9 15l3-3 3 3",
            "M12 12v9",
        ],
        Icon::Search => &["M11 3a8 8 0 1 1 0 16 8 8 0 0 1 0-16z", "M21 21l-4.3-4.3"],
        Icon::ChevronLeft => &["M15 18l-6-6 6-6"],
        Icon::ChevronRight => &["M9 18l6-6-6-6"],
        Icon::ChevronDown => &["M6 9l6 6 6-6"],
        Icon::QuickOpen => &[
            "M22 12h-6l-2 3h-4l-2-3H2",
            "M5.5 5.1L2 12v6a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2v-6l-3.5-6.9A2 2 0 0 0 16.8 4H7.2a2 2 0 0 0-1.7 1.1z",
        ],
        Icon::History => &[
            "M3 12a9 9 0 1 0 9-9 9.7 9.7 0 0 0-6.7 2.7L3 8",
            "M3 3v5h5",
            "M12 7v5l4 2",
        ],
        Icon::Layers => &[
            "M12.8 2.2a2 2 0 0 0-1.7 0L2.6 6.1a1 1 0 0 0 0 1.8l8.6 3.9a2 2 0 0 0 1.7 0l8.6-3.9a1 1 0 0 0 0-1.8z",
            "M22 17.6l-9.2 4.2a2 2 0 0 1-1.7 0L2 17.6",
            "M22 12.6l-9.2 4.2a2 2 0 0 1-1.7 0L2 12.6",
        ],
        Icon::Sliders => &[
            "M4 21v-7",
            "M4 10V3",
            "M12 21v-9",
            "M12 8V3",
            "M20 21v-5",
            "M20 12V3",
            "M2 14h4",
            "M10 8h4",
            "M18 16h4",
        ],
        Icon::User => &[
            "M12 3a5 5 0 1 1 0 10 5 5 0 0 1 0-10z",
            "M20 21a8 8 0 0 0-16 0",
        ],
        Icon::Bookmark => &["M19 21l-7-4-7 4V5a2 2 0 0 1 2-2h10a2 2 0 0 1 2 2z"],
        Icon::Plus => &["M5 12h14", "M12 5v14"],
        Icon::Check => &["M20 6L9 17l-5-5"],
    }
}

#[component]
pub fn Svg(icon: Icon, #[props(default = "size-4".to_owned())] class: String) -> Element {
    rsx! {
        svg {
            class: "{class} shrink-0",
            view_box: "0 0 24 24",
            fill: "none",
            stroke: "currentColor",
            stroke_width: "1.75",
            stroke_linecap: "round",
            stroke_linejoin: "round",
            for (i, d) in paths(icon).iter().enumerate() {
                path { key: "{i}", d: *d }
            }
        }
    }
}

//! [`IconName`]: the icon set, drawn from Lucide (ISC licence, see
//! `assets/icons/LICENSE`). Each variant names one bundled SVG.

use std::str::FromStr;

macro_rules! icon_names {
    ($($variant:ident => $file:literal,)*) => {
        /// A bundled icon. Parses from its variant name in snake_case
        /// (`"tool_hammer"`).
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum IconName {
            $($variant,)*
        }

        impl IconName {
            /// Every icon, in declaration order.
            pub const ALL: &'static [IconName] = &[$(IconName::$variant,)*];

            /// The variant name (`"ToolHammer"`).
            pub const fn name(self) -> &'static str {
                match self {
                    $(IconName::$variant => stringify!($variant),)*
                }
            }

            /// The asset path, e.g. `icons/hammer.svg`.
            pub const fn path(self) -> &'static str {
                match self {
                    $(IconName::$variant => concat!("icons/", $file, ".svg"),)*
                }
            }
        }

        /// `(asset path, SVG bytes)` for every bundled icon.
        pub(crate) const ICON_FILES: &[(&str, &[u8])] = &[
            $((
                concat!("icons/", $file, ".svg"),
                include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/assets/icons/", $file, ".svg")),
            ),)*
        ];
    };
}

icon_names! {
    Archive => "archive",
    ArrowCircle => "refresh-cw",
    ArrowDown => "arrow-down",
    ArrowLeft => "arrow-left",
    ArrowRight => "arrow-right",
    ArrowRightLeft => "arrow-right-left",
    ArrowUp => "arrow-up",
    ArrowUpRight => "arrow-up-right",
    Bell => "bell",
    Blocks => "blocks",
    Bookmark => "bookmark",
    Box => "box",
    BoxOpen => "package-open",
    CaseSensitive => "case-sensitive",
    Check => "check",
    ChevronDown => "chevron-down",
    ChevronLeft => "chevron-left",
    ChevronRight => "chevron-right",
    ChevronUp => "chevron-up",
    ChevronUpDown => "chevrons-up-down",
    Circle => "circle",
    CircleCheck => "circle-check",
    Close => "x",
    Clock => "clock",
    CloudDownload => "cloud-download",
    Code => "code",
    Copy => "copy",
    CursorIBeam => "text-cursor",
    Dash => "minus",
    DatabaseZap => "database-zap",
    Debug => "bug",
    DebugContinue => "play",
    DebugPause => "pause",
    DebugStepInto => "arrow-down-to-dot",
    DebugStepOut => "arrow-up-from-dot",
    DebugStepOver => "redo-dot",
    Download => "download",
    Ellipsis => "ellipsis",
    Eraser => "eraser",
    Error => "circle-alert",
    ExternalLink => "external-link",
    Eye => "eye",
    EyeOff => "eye-off",
    File => "file",
    FileTree => "folder-tree",
    Filter => "funnel",
    Folder => "folder",
    FolderOpen => "folder-open",
    FolderSearch => "folder-search",
    GitBranch => "git-branch",
    GitGraph => "git-graph",
    Hammer => "hammer",
    Hash => "hash",
    HistoryRerun => "history",
    Info => "info",
    Link => "link",
    LoadCircle => "loader-circle",
    Lock => "lock",
    MagnifyingGlass => "search",
    Pencil => "pencil",
    Person => "user",
    PlayFilled => "play-filled",
    Plus => "plus",
    Public => "globe",
    PullRequest => "git-pull-request",
    Regex => "regex",
    Replace => "replace",
    ReplaceAll => "replace-all",
    ReplaceNext => "replace",
    RotateCw => "rotate-cw",
    Server => "server",
    Settings => "settings",
    Share => "share-2",
    Sparkle => "sparkles",
    Stop => "square-filled",
    Terminal => "terminal",
    ThreadsSidebarLeftClosed => "panel-left-dashed",
    ThreadsSidebarLeftOpen => "panel-left",
    ToolHammer => "hammer",
    Trash => "trash-2",
    Undo => "undo-2",
    Warning => "triangle-alert",
    WholeWord => "whole-word",
    XCircle => "circle-x",
}

fn snake_case(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 4);
    for (i, c) in name.chars().enumerate() {
        if c.is_ascii_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.push(c.to_ascii_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// The text did not name an icon.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnknownIcon(pub String);

impl std::fmt::Display for UnknownIcon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unknown icon `{}`", self.0)
    }
}

impl std::error::Error for UnknownIcon {}

impl FromStr for IconName {
    type Err = UnknownIcon;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        IconName::ALL
            .iter()
            .copied()
            .find(|icon| snake_case(icon.name()) == s)
            .ok_or_else(|| UnknownIcon(s.to_owned()))
    }
}

impl std::fmt::Display for IconName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_snake_case_names() {
        assert_eq!("tool_hammer".parse(), Ok(IconName::ToolHammer));
        assert_eq!("x_circle".parse(), Ok(IconName::XCircle));
        assert!("ToolHammer".parse::<IconName>().is_err());
    }

    #[test]
    fn every_icon_has_a_bundled_svg() {
        for icon in IconName::ALL {
            let file = ICON_FILES.iter().find(|(path, _)| *path == icon.path());
            let (_, bytes) = file.expect("bundled");
            assert!(bytes.starts_with(b"<!--") || bytes.starts_with(b"<svg"));
        }
    }
}

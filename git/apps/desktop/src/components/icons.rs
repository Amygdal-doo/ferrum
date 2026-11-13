//! App icons, mapped onto the shared `ui` crate icon set so every Ferrum tool shares one look.

use ferrum_ui::components::IconName;

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
    ChevronUp,
    QuickOpen,
    History,
    Layers,
    Sliders,
    User,
    Bookmark,
    Plus,
    Close,
    Undo,
    File,
    Filter,
}

impl Icon {
    #[must_use]
    pub const fn name(self) -> IconName {
        match self {
            Self::Terminal => IconName::Terminal,
            Self::Folder => IconName::Folder,
            Self::FolderOpen => IconName::FolderOpen,
            Self::Refresh => IconName::RotateCw,
            Self::Pull => IconName::ArrowDown,
            Self::Push => IconName::ArrowUp,
            Self::Fetch => IconName::CloudDownload,
            Self::Sync => IconName::ArrowRightLeft,
            Self::Merge => IconName::PullRequest,
            Self::Rebase => IconName::GitGraph,
            Self::Branch => IconName::GitBranch,
            Self::Tag => IconName::Hash,
            Self::Cloud => IconName::Server,
            Self::Archive => IconName::Archive,
            Self::ArchiveRestore => IconName::BoxOpen,
            Self::Search => IconName::MagnifyingGlass,
            Self::ChevronLeft => IconName::ChevronLeft,
            Self::ChevronRight => IconName::ChevronRight,
            Self::ChevronDown => IconName::ChevronDown,
            Self::ChevronUp => IconName::ChevronUp,
            Self::QuickOpen => IconName::FolderSearch,
            Self::History => IconName::Clock,
            Self::Layers => IconName::Box,
            Self::Sliders => IconName::Settings,
            Self::User => IconName::Person,
            Self::Bookmark => IconName::Bookmark,
            Self::Plus => IconName::Plus,
            Self::Close => IconName::Close,
            Self::Undo => IconName::Undo,
            Self::File => IconName::File,
            Self::Filter => IconName::Filter,
        }
    }

    #[must_use]
    pub fn icon(self) -> ferrum_ui::components::Icon {
        ferrum_ui::components::Icon::new(self.name())
    }
}

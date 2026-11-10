//! Single source of truth for site-wide constants and links.
//! Pages must read links from here instead of hard-coding them.

pub const SITE_NAME: &str = "Ferrum";
pub const SITE_TAGLINE: &str = "One toolbox, every tool.";
pub const SITE_DESCRIPTION: &str = "Ferrum is a family of native developer tools written in Rust and \
     rendered with GPUI: an IDE, an API client, a Git client, a database IDE, a terminal, a \
     container manager and developer utilities, installed and updated by Ferrum Toolbox.";

/// Every download CTA on the site links here.
pub const DOWNLOAD_PATH: &str = "/download";
pub const DOWNLOAD_LABEL: &str = "Download";

/// Public, releases-only repository. Every Ferrum app release lives here as
/// `<app>-v<semver>` (`toolbox-v…`, `api-v…`, …); no GitHub account is needed.
pub const REPO_SLUG: &str = "Amygdal-doo/ferrum-releases";
/// Human-facing release list. Also the fallback for [`MACOS_DOWNLOAD_PATH`].
pub const RELEASES_URL: &str = "https://github.com/Amygdal-doo/ferrum-releases/releases";
/// Public GitHub API listing used to resolve the newest toolbox installer.
pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/Amygdal-doo/ferrum-releases/releases?per_page=100";

/// Redirects (302) to the newest stable Ferrum Toolbox DMG (Apple Silicon).
pub const MACOS_DOWNLOAD_PATH: &str = "/download/macos";

/// One-line Terminal install. curl downloads aren't quarantined, so this
/// skips the Gatekeeper warning a browser-downloaded DMG gets.
pub const MACOS_INSTALL_COMMAND: &str = "curl -fsSL https://raw.githubusercontent.com/Amygdal-doo/ferrum-releases/main/install.sh | bash";

/// Whether current macOS builds are Developer ID signed and notarized.
/// While `false`, the download page explains Gatekeeper's "Open Anyway" step.
/// Flip to `true` once notarized builds ship; that hides the section.
pub const MACOS_NOTARIZED: bool = true;

/// PLACEHOLDER: contact address (none configured).
pub const CONTACT_EMAIL: Option<&str> = None;

/// Upstream project credited by Ferrum IDE. Text credit only; no Zed logos.
pub const ZED_UPSTREAM_URL: &str = "https://github.com/zed-industries/zed";
/// Amygdal, the studio that builds Ferrum.
pub const AMYGDAL_URL: &str = "https://amygdal.com";
/// GPUI, the UI framework every Ferrum desktop app is built on.
pub const GPUI_URL: &str = "https://gpui.rs";

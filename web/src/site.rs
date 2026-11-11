//! Single source of truth for site-wide constants and links.
//! Pages must read links from here instead of hard-coding them.

pub const SITE_NAME: &str = "Ferrum";
pub const SITE_TAGLINE: &str = "One launcher, every tool.";
/// Home page `<title>` after "Ferrum: ".
pub const SITE_TITLE: &str = "native developer tools for macOS, built in Rust";
pub const SITE_DESCRIPTION: &str = "Ferrum is a family of native developer tools written in Rust and \
     rendered with GPUI: a code editor, an API client, a Git client, a database IDE, a terminal, a \
     container manager and developer utilities, installed and updated by Ferrum Launcher.";

/// Every download CTA on the site links here.
pub const DOWNLOAD_PATH: &str = "/download";
pub const DOWNLOAD_LABEL: &str = "Download Launcher";

/// Public, releases-only repository. Every Ferrum app release lives here as
/// `<app>-v<semver>` (`launcher-v…`, `api-v…`, …); no GitHub account is needed.
pub const REPO_SLUG: &str = "Amygdal-doo/ferrum-releases";
/// Human-facing release list. Also the fallback for [`MACOS_DOWNLOAD_PATH`].
pub const RELEASES_URL: &str = "https://github.com/Amygdal-doo/ferrum-releases/releases";
/// Public GitHub API listing used to resolve the newest launcher installer.
pub const RELEASES_API_URL: &str =
    "https://api.github.com/repos/Amygdal-doo/ferrum-releases/releases?per_page=100";

/// Redirects (302) to the newest stable Ferrum Launcher DMG (Apple Silicon).
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

/// Upstream of GPUI and the `ui`/`theme` crates every Ferrum app is built on.
/// Text credit only; no Zed logos.
pub const ZED_UPSTREAM_URL: &str = "https://github.com/zed-industries/zed";
/// Amygdal, the studio that builds Ferrum.
pub const AMYGDAL_URL: &str = "https://amygdal.com";
/// GPUI, the UI framework every Ferrum desktop app is built on.
pub const GPUI_URL: &str = "https://gpui.rs";

/// The company that publishes Ferrum (JSON-LD `Organization`, signing identity).
pub const ORGANIZATION_LEGAL_NAME: &str = "AMYGDAL d.o.o.";
pub const ORGANIZATION_NAME: &str = "Amygdal";
pub const ORGANIZATION_CITY: &str = "Sarajevo";
/// ISO 3166-1 alpha-2: Bosnia and Herzegovina.
pub const ORGANIZATION_COUNTRY: &str = "BA";

/// Date the static page copy was last revised (`YYYY-MM-DD`): the sitemap's
/// `lastmod` for pages without a date of their own. Bump it with content edits.
pub const CONTENT_UPDATED: &str = "2026-11-11";

/// Raw `install.sh` used by [`MACOS_INSTALL_COMMAND`].
pub const INSTALL_SCRIPT_URL: &str =
    "https://raw.githubusercontent.com/Amygdal-doo/ferrum-releases/main/install.sh";

//! Single source of truth for site-wide constants and links.
//! Pages must read links from here instead of hard-coding them.

pub const SITE_NAME: &str = "Ferrum";
pub const SITE_TAGLINE: &str = "One toolbox, every tool.";
pub const SITE_DESCRIPTION: &str = "Ferrum is a family of native developer tools written in Rust and \
     rendered with GPUI: an IDE, an API client, a Git client, a database IDE and a terminal, \
     installed and updated by Ferrum Toolbox.";

/// Every download CTA on the site links here.
pub const DOWNLOAD_PATH: &str = "/download";
pub const DOWNLOAD_LABEL: &str = "Download";

/// The private monorepo. Shown as text on the download page only (team access),
/// never as a public "source" link.
pub const REPO_SLUG: &str = "Amygdal-doo/ferrum";
/// GitHub Releases of the monorepo: one release per app version (`<app>-v<semver>`).
/// Visible only to people with access to the repository.
pub const RELEASES_URL: &str = "https://github.com/Amygdal-doo/ferrum/releases";
/// Where to create the fine-grained token Ferrum Toolbox can use instead of `gh`.
pub const TOKEN_SETTINGS_URL: &str = "https://github.com/settings/personal-access-tokens/new";

/// PLACEHOLDER: contact address (none configured).
pub const CONTACT_EMAIL: Option<&str> = None;

/// Upstream project credited by Ferrum IDE. Text credit only; no Zed logos.
pub const ZED_UPSTREAM_URL: &str = "https://github.com/zed-industries/zed";
/// GPUI, the UI framework every Ferrum desktop app is built on.
pub const GPUI_URL: &str = "https://gpui.rs";

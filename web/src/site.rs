//! Single source of truth for site-wide constants and ALL placeholder links.
//!
//! Anything marked `PLACEHOLDER` is not real yet and must be replaced before
//! launch. Pages must read links from here instead of hard-coding them.

pub const SITE_NAME: &str = "Ferrum";
pub const SITE_TAGLINE: &str = "Developer tools forged in Rust.";
pub const SITE_DESCRIPTION: &str = "Ferrum is a family of fast, local-first developer tools written in Rust: \
     FerrumData (database IDE), FerrumAPI (API client) and Ferrum IDE (code editor).";

/// PLACEHOLDER: GitHub organisation URL (not created yet).
pub const GITHUB_ORG_URL: &str = "https://github.com/your-org";
/// PLACEHOLDER: per-product repositories (not public yet).
pub const GITHUB_FERRUMDATA_URL: &str = "https://github.com/your-org/ferrumdata";
pub const GITHUB_FERRUMAPI_URL: &str = "https://github.com/your-org/ferrumapi";
pub const GITHUB_FERRUM_IDE_URL: &str = "https://github.com/your-org/ferrum-ide";

/// Downloads do not exist yet; every download CTA renders this label, disabled.
pub const DOWNLOAD_LABEL: &str = "Coming soon";
/// PLACEHOLDER: contact address (none configured).
pub const CONTACT_EMAIL: Option<&str> = None;

/// Upstream project credited by Ferrum IDE. Text credit only; no Zed logos.
pub const ZED_UPSTREAM_URL: &str = "https://github.com/zed-industries/zed";

/// True when a URL above is still a placeholder, so templates can label it.
pub fn is_placeholder(url: &str) -> bool {
    url.contains("your-org")
}

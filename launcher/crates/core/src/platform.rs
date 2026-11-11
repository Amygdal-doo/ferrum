//! Per-OS conventions: executable names, CLI shims, desktop integration and
//! archive formats. The helpers are pure so every platform's behaviour is
//! testable on any host.

use crate::catalog::{AppId, AppInfo};
use std::path::Path;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Platform {
    MacOs,
    Linux,
    Windows,
    Other,
}

impl Platform {
    #[must_use]
    pub fn current() -> Self {
        if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Other
        }
    }

    /// Platform of a Rust target triple.
    #[must_use]
    pub fn of_target(target: &str) -> Self {
        if target.ends_with("-apple-darwin") {
            Self::MacOs
        } else if target.contains("-linux-") {
            Self::Linux
        } else if target.contains("-windows-") {
            Self::Windows
        } else {
            Self::Other
        }
    }

    /// `binary` with the platform's executable suffix.
    #[must_use]
    pub fn executable(self, binary: &str) -> String {
        match self {
            Self::Windows => format!("{binary}.exe"),
            _ => binary.to_string(),
        }
    }

    /// Name of the CLI entry point in `~/.ferrum/bin`.
    #[must_use]
    pub fn cli_entry(self, binary: &str) -> String {
        match self {
            Self::Windows => format!("{binary}.cmd"),
            _ => binary.to_string(),
        }
    }
}

/// Archive formats the launcher can unpack.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArchiveKind {
    TarGz,
    Zip,
}

impl ArchiveKind {
    /// Kind from a release asset name.
    #[must_use]
    pub fn from_asset_name(name: &str) -> Option<Self> {
        if name.ends_with(".tar.gz") {
            Some(Self::TarGz)
        } else if name.ends_with(".zip") {
            Some(Self::Zip)
        } else {
            None
        }
    }
}

/// A Windows `.cmd` shim that forwards all arguments to `target`.
#[must_use]
pub fn cmd_shim(target: &Path) -> String {
    format!("@echo off\r\n\"{}\" %*\r\n", target.display())
}

/// Release version of the running binary. Workspace crates all build as
/// `CARGO_PKG_VERSION`, so the real version comes from `FERRUM_VERSION` set at
/// build time, or from the `FerrumVersion` key `scripts/bundle-macos.sh`
/// writes into the enclosing `.app`'s Info.plist.
#[must_use]
pub fn running_version() -> Option<semver::Version> {
    if let Some(version) = option_env!("FERRUM_VERSION") {
        return semver::Version::parse(version).ok();
    }
    let executable = std::env::current_exe().ok()?;
    let plist = executable.parent()?.parent()?.join("Info.plist");
    let contents = std::fs::read_to_string(plist).ok()?;
    plist_ferrum_version(&contents)
}

fn plist_ferrum_version(plist: &str) -> Option<semver::Version> {
    let after_key = plist.split_once("<key>FerrumVersion</key>")?.1;
    let value = after_key
        .split_once("<string>")?
        .1
        .split_once("</string>")?
        .0;
    semver::Version::parse(value.trim()).ok()
}

/// Desktop-entry file name for `app` (`~/.local/share/applications/`).
#[must_use]
pub fn desktop_entry_name(app: AppId) -> String {
    format!("ferrum-{app}.desktop")
}

/// Icon theme name for `app` (`hicolor/256x256/apps/<name>.png`).
#[must_use]
pub fn icon_name(app: AppId) -> String {
    format!("ferrum-{app}")
}

/// A freedesktop.org `.desktop` entry that launches `executable`.
#[must_use]
pub fn desktop_entry(info: &AppInfo, executable: &Path) -> String {
    let exec = executable.display().to_string().replace('"', "\\\"");
    format!(
        "[Desktop Entry]\n\
         Type=Application\n\
         Version=1.5\n\
         Name={name}\n\
         Comment={comment}\n\
         Exec=\"{exec}\" %U\n\
         Icon={icon}\n\
         Terminal=false\n\
         Categories=Development;\n\
         StartupWMClass={binary}\n",
        name = info.name,
        comment = info.description,
        icon = icon_name(info.id),
        binary = info.binary,
    )
}

/// The brand icon (256px PNG) compiled into the launcher for desktop
/// entries; `None` for apps this build doesn't know (their catalog logo is
/// used instead, once cached).
#[must_use]
pub fn icon_png(app: AppId) -> Option<&'static [u8]> {
    let png: &'static [u8] = match app.as_str() {
        "api" => include_bytes!("../../../../brand/png/api-256.png"),
        "git" => include_bytes!("../../../../brand/png/git-256.png"),
        "data" => include_bytes!("../../../../brand/png/data-256.png"),
        "term" => include_bytes!("../../../../brand/png/term-256.png"),
        "kit" => include_bytes!("../../../../brand/png/kit-256.png"),
        "dock" => include_bytes!("../../../../brand/png/dock-256.png"),
        "editor" => include_bytes!("../../../../brand/png/editor-256.png"),
        "launcher" => include_bytes!("../../../../brand/png/launcher-256.png"),
        _ => return None,
    };
    Some(png)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn platform_of_target() {
        assert_eq!(Platform::of_target("aarch64-apple-darwin"), Platform::MacOs);
        assert_eq!(
            Platform::of_target("x86_64-unknown-linux-gnu"),
            Platform::Linux
        );
        assert_eq!(
            Platform::of_target("x86_64-pc-windows-msvc"),
            Platform::Windows
        );
        assert_eq!(
            Platform::of_target("wasm32-unknown-unknown"),
            Platform::Other
        );
    }

    #[test]
    fn executable_and_cli_names() {
        assert_eq!(Platform::Windows.executable("ferrumapi"), "ferrumapi.exe");
        assert_eq!(Platform::Linux.executable("ferrumapi"), "ferrumapi");
        assert_eq!(Platform::Windows.cli_entry("fterm"), "fterm.cmd");
        assert_eq!(Platform::MacOs.cli_entry("fterm"), "fterm");
    }

    #[test]
    fn archive_kind_from_name() {
        assert_eq!(
            ArchiveKind::from_asset_name("api-1.0.0-x86_64-pc-windows-msvc.zip"),
            Some(ArchiveKind::Zip)
        );
        assert_eq!(
            ArchiveKind::from_asset_name("api-1.0.0-aarch64-apple-darwin.app.tar.gz"),
            Some(ArchiveKind::TarGz)
        );
        assert_eq!(
            ArchiveKind::from_asset_name("FerrumAPI-1.0.0-x86_64-setup.exe"),
            None
        );
    }

    #[test]
    fn cmd_shim_quotes_path_and_forwards_args() {
        let shim = cmd_shim(&PathBuf::from(r"C:\Users\me\AppData\Local\x\ferrumapi.exe"));
        assert_eq!(
            shim,
            "@echo off\r\n\"C:\\Users\\me\\AppData\\Local\\x\\ferrumapi.exe\" %*\r\n"
        );
    }

    #[test]
    fn desktop_entry_points_at_executable_and_icon() {
        let info = AppInfo::builtin(AppId::DATA).expect("data");
        let entry = desktop_entry(&info, Path::new("/home/me/.local/share/x/ferrumdata"));
        assert!(entry.starts_with("[Desktop Entry]\n"));
        assert!(entry.contains("Name=FerrumData\n"));
        assert!(entry.contains("Exec=\"/home/me/.local/share/x/ferrumdata\" %U\n"));
        assert!(entry.contains("Icon=ferrum-data\n"));
        assert_eq!(desktop_entry_name(AppId::DATA), "ferrum-data.desktop");
    }

    #[test]
    fn reads_ferrum_version_from_info_plist() {
        let plist = "<dict>\n  <key>CFBundleVersion</key>\n  <string>0.2.0</string>\n  \
                     <key>FerrumVersion</key>\n  <string>0.2.0-beta.1</string>\n</dict>";
        assert_eq!(
            plist_ferrum_version(plist),
            Some(semver::Version::parse("0.2.0-beta.1").expect("semver"))
        );
        assert_eq!(plist_ferrum_version("<dict></dict>"), None);
        assert_eq!(
            plist_ferrum_version("<key>FerrumVersion</key><string>nope</string>"),
            None
        );
    }

    #[test]
    fn every_builtin_app_has_a_png_icon() {
        for app in AppId::BUILTIN {
            assert!(
                icon_png(app).is_some_and(|png| png.starts_with(b"\x89PNG")),
                "{app}"
            );
        }
        assert!(icon_png(AppId::new("forge").expect("id")).is_none());
        assert!(
            icon_png(AppId::new("ide").expect("id")).is_none(),
            "retired"
        );
    }
}

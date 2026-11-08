//! Verifying and unpacking release archives into the version store.

use crate::platform::ArchiveKind;
use crate::{Result, ToolboxError};
use flate2::read::GzDecoder;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

/// Fails closed when `actual` doesn't match the published checksum.
pub fn verify_checksum(asset: &str, checksum_file: &str, actual: &str) -> Result<()> {
    let expected = crate::release::parse_checksum(checksum_file)
        .ok_or_else(|| ToolboxError::InvalidChecksum(asset.to_string()))?;
    if expected == actual.to_ascii_lowercase() {
        Ok(())
    } else {
        Err(ToolboxError::ChecksumMismatch {
            asset: asset.to_string(),
            expected,
            actual: actual.to_string(),
        })
    }
}

/// A relative path that stays inside the extraction root.
fn is_safe_relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

/// The single `*.app` bundle at the root of an installed version, if any.
#[must_use]
pub fn locate_bundle(version_dir: &Path) -> Option<PathBuf> {
    let mut bundles = std::fs::read_dir(version_dir)
        .ok()?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "app") && path.is_dir());
    let bundle = bundles.next()?;
    bundles.next().is_none().then_some(bundle)
}

/// `binary` (or `binary.exe`) at the root of `version_dir`, or inside its
/// `.app` bundle.
#[must_use]
pub fn locate_binary(version_dir: &Path, binary: &str) -> Option<PathBuf> {
    for name in [binary.to_string(), format!("{binary}.exe")] {
        let plain = version_dir.join(name);
        if plain.is_file() {
            return Some(plain);
        }
    }
    let bundled = locate_bundle(version_dir)?
        .join("Contents")
        .join("MacOS")
        .join(binary);
    bundled.is_file().then_some(bundled)
}

/// Unpacks `archive` (`.tar.gz` or `.zip`) into a fresh temp dir next to `destination`,
/// checks `binary` exists (at its root or in its `.app` bundle), then moves it into place, replacing
/// any previous install of the same version. Entries that would escape the
/// directory (absolute paths, `..`, or links pointing outside) abort the install.
pub fn install_archive(
    archive: &Path,
    kind: ArchiveKind,
    destination: &Path,
    binary: &str,
) -> Result<()> {
    let parent = destination
        .parent()
        .ok_or_else(|| ToolboxError::Invalid("install path has no parent".into()))?;
    std::fs::create_dir_all(parent)?;
    let staging = parent.join(format!(".staging-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&staging)?;

    let extracted = match kind {
        ArchiveKind::TarGz => extract(archive, &staging),
        ArchiveKind::Zip => extract_zip(archive, &staging),
    };
    let result = extracted.and_then(|()| {
        if locate_binary(&staging, binary).is_none() {
            return Err(ToolboxError::MissingBinary(binary.to_string()));
        }
        replace_dir(&staging, destination)
    });
    if result.is_err()
        && staging.exists()
        && let Err(e) = std::fs::remove_dir_all(&staging)
    {
        tracing::warn!(error = %e, "could not remove staging directory");
    }
    result
}

fn extract(archive: &Path, into: &Path) -> Result<()> {
    let mut tar = tar::Archive::new(GzDecoder::new(File::open(archive)?));
    tar.set_preserve_permissions(true);
    tar.set_overwrite(true);
    for entry in tar.entries()? {
        let mut entry = entry?;
        let path: PathBuf = entry.path()?.into_owned();
        if !is_safe_relative(&path) {
            return Err(ToolboxError::UnsafeArchiveEntry(path.display().to_string()));
        }
        let kind = entry.header().entry_type();
        if (kind.is_symlink() || kind.is_hard_link())
            && let Some(target) = entry.link_name()?
        {
            let resolved = path
                .parent()
                .map(|dir| dir.join(&target))
                .unwrap_or_else(|| target.to_path_buf());
            if target.is_absolute() || !stays_inside(&resolved) {
                return Err(ToolboxError::UnsafeArchiveEntry(format!(
                    "{} -> {}",
                    path.display(),
                    target.display()
                )));
            }
        }
        if !entry.unpack_in(into)? {
            return Err(ToolboxError::UnsafeArchiveEntry(path.display().to_string()));
        }
    }
    Ok(())
}

/// Zip counterpart of [`extract`]. Links are rejected outright: Windows
/// archives never need them.
fn extract_zip(archive: &Path, into: &Path) -> Result<()> {
    let mut zip = zip::ZipArchive::new(File::open(archive)?)
        .map_err(|e| ToolboxError::Invalid(format!("invalid zip archive: {e}")))?;
    for index in 0..zip.len() {
        let mut entry = zip
            .by_index(index)
            .map_err(|e| ToolboxError::Invalid(format!("invalid zip entry: {e}")))?;
        let name = entry.name().to_string();
        let Some(relative) = entry.enclosed_name().filter(|path| is_safe_relative(path)) else {
            return Err(ToolboxError::UnsafeArchiveEntry(name));
        };
        if entry.is_symlink() {
            return Err(ToolboxError::UnsafeArchiveEntry(name));
        }
        let path = into.join(relative);
        if entry.is_dir() {
            std::fs::create_dir_all(&path)?;
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = File::create(&path)?;
        std::io::copy(&mut entry, &mut file)?;
        #[cfg(unix)]
        if let Some(mode) = entry.unix_mode() {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(mode & 0o777))?;
        }
    }
    Ok(())
}

/// Whether a relative path (possibly with `..`) never climbs above its root.
fn stays_inside(path: &Path) -> bool {
    let mut depth = 0i32;
    for component in path.components() {
        match component {
            Component::Normal(_) => depth += 1,
            Component::CurDir => {}
            Component::ParentDir => {
                depth -= 1;
                if depth < 0 {
                    return false;
                }
            }
            Component::RootDir | Component::Prefix(_) => return false,
        }
    }
    true
}

/// Swaps `staging` into `destination` with renames so a crash never leaves
/// a half-written version directory.
fn replace_dir(staging: &Path, destination: &Path) -> Result<()> {
    if destination.exists() {
        let old = destination.with_extension(format!("old-{}", uuid::Uuid::new_v4()));
        std::fs::rename(destination, &old)?;
        std::fs::rename(staging, destination)?;
        std::fs::remove_dir_all(&old)?;
    } else {
        std::fs::rename(staging, destination)?;
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use flate2::Compression;
    use flate2::write::GzEncoder;

    pub(crate) fn make_archive(path: &Path, files: &[(&str, &[u8], u32)]) {
        let encoder = GzEncoder::new(File::create(path).expect("create"), Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        for (name, data, mode) in files {
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(*mode);
            header.set_cksum();
            builder
                .append_data(&mut header, name, *data)
                .expect("append");
        }
        builder.into_inner().expect("tar").finish().expect("gzip");
    }

    /// `tar::Builder` refuses `..` paths, so write the raw header ourselves.
    fn make_raw_archive(path: &Path, name: &str) {
        let encoder = GzEncoder::new(File::create(path).expect("create"), Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_old();
        let bytes = name.as_bytes();
        header.as_old_mut().name[..bytes.len()].copy_from_slice(bytes);
        header.set_size(1);
        header.set_mode(0o644);
        header.set_entry_type(tar::EntryType::Regular);
        header.set_cksum();
        builder.append(&header, &b"x"[..]).expect("append");
        builder.into_inner().expect("tar").finish().expect("gzip");
    }

    #[test]
    fn checksum_verification_fails_closed() {
        let good = "b".repeat(64);
        assert!(verify_checksum("a", &format!("{good}  a"), &good).is_ok());
        assert!(matches!(
            verify_checksum("a", &good, &"c".repeat(64)),
            Err(ToolboxError::ChecksumMismatch { .. })
        ));
        assert!(matches!(
            verify_checksum("a", "not a checksum", &good),
            Err(ToolboxError::InvalidChecksum(_))
        ));
    }

    #[test]
    fn installs_archive_and_keeps_exec_bit() {
        let dir = tempfile::tempdir().expect("tmp");
        let archive = dir.path().join("app.tar.gz");
        make_archive(
            &archive,
            &[
                ("ferrumapi", b"#!/bin/sh\n", 0o755),
                ("README.txt", b"hi", 0o644),
            ],
        );
        let destination = dir.path().join("apps/api/0.2.0");
        install_archive(&archive, ArchiveKind::TarGz, &destination, "ferrumapi").expect("install");
        let binary = destination.join("ferrumapi");
        assert!(binary.is_file());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&binary)
                .expect("meta")
                .permissions()
                .mode();
            assert_eq!(mode & 0o111, 0o111);
        }
        // Reinstalling the same version replaces it cleanly.
        make_archive(&archive, &[("ferrumapi", b"v2", 0o755)]);
        install_archive(&archive, ArchiveKind::TarGz, &destination, "ferrumapi")
            .expect("reinstall");
        assert!(!destination.join("README.txt").exists());
        let leftovers: Vec<_> = std::fs::read_dir(dir.path().join("apps/api"))
            .expect("read")
            .filter_map(|e| e.ok())
            .collect();
        assert_eq!(leftovers.len(), 1);
    }

    #[test]
    fn rejects_path_traversal() {
        let dir = tempfile::tempdir().expect("tmp");
        let archive = dir.path().join("evil.tar.gz");
        make_raw_archive(&archive, "../escaped");
        let destination = dir.path().join("apps/api/1.0.0");
        let result = install_archive(&archive, ArchiveKind::TarGz, &destination, "ferrumapi");
        assert!(matches!(result, Err(ToolboxError::UnsafeArchiveEntry(_))));
        assert!(!dir.path().join("apps/escaped").exists());
        assert!(!destination.exists());
        let staging_left = std::fs::read_dir(dir.path().join("apps/api"))
            .expect("read")
            .count();
        assert_eq!(staging_left, 0);
    }

    #[test]
    fn rejects_links_out_of_the_root() {
        let dir = tempfile::tempdir().expect("tmp");
        let archive = dir.path().join("link.tar.gz");
        let encoder = GzEncoder::new(File::create(&archive).expect("create"), Compression::fast());
        let mut builder = tar::Builder::new(encoder);
        let mut header = tar::Header::new_gnu();
        header.set_entry_type(tar::EntryType::Symlink);
        header.set_size(0);
        header.set_mode(0o777);
        builder
            .append_link(&mut header, "ferrumapi", "../../../../etc/passwd")
            .expect("append");
        builder.into_inner().expect("tar").finish().expect("gzip");
        let result = install_archive(
            &archive,
            ArchiveKind::TarGz,
            &dir.path().join("apps/api/1.0.0"),
            "ferrumapi",
        );
        assert!(matches!(result, Err(ToolboxError::UnsafeArchiveEntry(_))));
    }

    #[test]
    fn installs_app_bundle_archive() {
        let dir = tempfile::tempdir().expect("tmp");
        let archive = dir.path().join("bundle.tar.gz");
        make_archive(
            &archive,
            &[
                ("FerrumData.app/Contents/Info.plist", b"<plist/>", 0o644),
                (
                    "FerrumData.app/Contents/MacOS/ferrumdata",
                    b"#!/bin/sh\n",
                    0o755,
                ),
            ],
        );
        let destination = dir.path().join("apps/data/0.3.0");
        install_archive(&archive, ArchiveKind::TarGz, &destination, "ferrumdata").expect("install");
        let bundle = locate_bundle(&destination).expect("bundle");
        assert_eq!(bundle, destination.join("FerrumData.app"));
        assert_eq!(
            locate_binary(&destination, "ferrumdata"),
            Some(bundle.join("Contents/MacOS/ferrumdata"))
        );
        assert_eq!(locate_binary(&destination, "fterm"), None);

        make_archive(
            &archive,
            &[("FerrumData.app/Contents/MacOS/other", b"x", 0o755)],
        );
        let result = install_archive(
            &archive,
            ArchiveKind::TarGz,
            &dir.path().join("apps/data/0.4.0"),
            "ferrumdata",
        );
        assert!(matches!(result, Err(ToolboxError::MissingBinary(_))));
    }

    #[test]
    fn requires_binary_at_root() {
        let dir = tempfile::tempdir().expect("tmp");
        let archive = dir.path().join("app.tar.gz");
        make_archive(&archive, &[("bin/ferrumapi", b"x", 0o755)]);
        let result = install_archive(
            &archive,
            ArchiveKind::TarGz,
            &dir.path().join("apps/api/1.0.0"),
            "ferrumapi",
        );
        assert!(matches!(result, Err(ToolboxError::MissingBinary(_))));
    }

    fn make_zip(path: &Path, files: &[(&str, &[u8])]) {
        use std::io::Write;
        let mut zip = zip::ZipWriter::new(File::create(path).expect("create"));
        let options = zip::write::SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .unix_permissions(0o755);
        for (name, data) in files {
            zip.start_file(*name, options).expect("start");
            zip.write_all(data).expect("write");
        }
        zip.finish().expect("finish");
    }

    #[test]
    fn installs_windows_zip_and_finds_exe() {
        let dir = tempfile::tempdir().expect("tmp");
        let archive = dir.path().join("api.zip");
        make_zip(&archive, &[("ferrumapi.exe", b"MZ"), ("api.ico", b"ico")]);
        let destination = dir.path().join("apps/api/0.2.0");
        install_archive(&archive, ArchiveKind::Zip, &destination, "ferrumapi").expect("install");
        assert_eq!(
            locate_binary(&destination, "ferrumapi"),
            Some(destination.join("ferrumapi.exe"))
        );
        assert!(destination.join("api.ico").is_file());
    }

    #[test]
    fn rejects_zip_path_traversal() {
        let dir = tempfile::tempdir().expect("tmp");
        let archive = dir.path().join("evil.zip");
        make_zip(
            &archive,
            &[("../escaped.exe", b"MZ"), ("ferrumapi.exe", b"MZ")],
        );
        let result = install_archive(
            &archive,
            ArchiveKind::Zip,
            &dir.path().join("apps/api/1.0.0"),
            "ferrumapi",
        );
        assert!(matches!(result, Err(ToolboxError::UnsafeArchiveEntry(_))));
        assert!(!dir.path().join("apps/escaped.exe").exists());
        assert!(!dir.path().join("escaped.exe").exists());
    }

    #[test]
    fn path_safety_rules() {
        assert!(is_safe_relative(Path::new("a/b")));
        assert!(!is_safe_relative(Path::new("/etc/passwd")));
        assert!(!is_safe_relative(Path::new("a/../../b")));
        assert!(stays_inside(Path::new("a/../b")));
        assert!(!stays_inside(Path::new("a/../../b")));
    }
}

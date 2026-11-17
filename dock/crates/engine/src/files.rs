//! A container's file system through the engine's archive endpoints: file
//! contents and downloads from the tar of a file, uploads as a one-file tar,
//! and folder listings from the tar of a folder.
//!
//! The archive of a folder holds everything below it, and the engine keeps
//! building it after the reader gave up (a cancelled archive of `/` held up
//! the next archive call for 30 s on Docker Desktop). So a running container
//! with a shell is listed with one `exec` of [`LIST_SCRIPT`] (POSIX `sh`,
//! machine-readable records, no `ls` output parsing); the archive is the
//! fallback for stopped containers and images without a shell, reading at
//! most [`LIST_BYTE_LIMIT`] bytes and [`LIST_ENTRY_LIMIT`] entries and saying
//! when it stopped early ([`DirListing::partial`]).

use crate::archive::{EntryKind, TarEvent, TarReader};
use crate::engine::SharedEngine;
use crate::error::{EngineError, EngineResult};
use futures::StreamExt;
use std::path::Path;

/// Bytes of a folder's archive read for one listing.
pub const LIST_BYTE_LIMIT: u64 = 64 << 20;
/// Entries listed at most.
pub const LIST_ENTRY_LIMIT: usize = 5_000;
/// Bytes of a file shown in the viewer.
pub const VIEW_LIMIT: u64 = 1 << 20;
/// Largest file uploaded (it is sent in one request).
pub const UPLOAD_LIMIT: u64 = 512 << 20;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub name: String,
    pub kind: EntryKind,
    pub size: u64,
    pub mode: u32,
    pub mtime: i64,
    /// Symlink target.
    pub link: Option<String>,
}

impl FileEntry {
    /// `drwxr-xr-x`-style permissions.
    #[must_use]
    pub fn mode_string(&self) -> String {
        let kind = match self.kind {
            EntryKind::Dir => 'd',
            EntryKind::Symlink => 'l',
            EntryKind::File => '-',
            EntryKind::Other => '?',
        };
        let bits = "rwxrwxrwx";
        std::iter::once(kind)
            .chain(bits.chars().enumerate().map(|(i, c)| {
                if self.mode & (0o400 >> i) == 0 {
                    '-'
                } else {
                    c
                }
            }))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DirListing {
    pub path: String,
    /// Folders first, then by name.
    pub entries: Vec<FileEntry>,
    /// The archive was too big to read to the end: entries may be missing.
    pub partial: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileContent {
    pub path: String,
    pub size: u64,
    pub data: Vec<u8>,
    /// Only the first [`VIEW_LIMIT`] bytes were read.
    pub truncated: bool,
}

impl FileContent {
    /// The text, or `None` for a binary file (a NUL byte, or not UTF-8 apart
    /// from a character cut at the end of a truncated read).
    #[must_use]
    pub fn text(&self) -> Option<&str> {
        if self.data.contains(&0) {
            return None;
        }
        match std::str::from_utf8(&self.data) {
            Ok(text) => Some(text),
            Err(e) if self.truncated && e.error_len().is_none() => {
                std::str::from_utf8(&self.data[..e.valid_up_to()]).ok()
            }
            Err(_) => None,
        }
    }
}

/// An absolute path without `.`, `..`, empty or trailing parts (`/` for none).
#[must_use]
pub fn normalize(path: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            other => parts.push(other),
        }
    }
    format!("/{}", parts.join("/"))
}

/// The folder above `path` (`/` stays `/`).
#[must_use]
pub fn parent(path: &str) -> String {
    normalize(&format!("{path}/.."))
}

#[must_use]
pub fn join(dir: &str, name: &str) -> String {
    normalize(&format!("{dir}/{name}"))
}

/// Where a link in `dir` pointing at `target` leads (absolute targets stay).
#[must_use]
pub fn resolve(dir: &str, target: &str) -> String {
    if target.starts_with('/') {
        normalize(target)
    } else {
        join(dir, target)
    }
}

/// The last part of `path` (`/` for the root).
#[must_use]
pub fn base_name(path: &str) -> String {
    let path = normalize(path);
    path.rsplit('/')
        .next()
        .filter(|n| !n.is_empty())
        .unwrap_or("/")
        .to_owned()
}

/// An archive name without `./`, leading or trailing slashes.
fn clean(name: &str) -> &str {
    name.trim_start_matches("./")
        .trim_start_matches('/')
        .trim_end_matches('/')
}

/// `name` relative to the archive's `root` entry, when it is a direct child.
fn child_name(root: &str, name: &str) -> Option<String> {
    let name = clean(name);
    let rel = if root.is_empty() || root == "." {
        name
    } else {
        name.strip_prefix(root)?.strip_prefix('/')?
    };
    (!rel.is_empty() && !rel.contains('/')).then(|| rel.to_owned())
}

fn not_a(what: &str, path: &str) -> EngineError {
    EngineError::Api {
        status: 400,
        message: format!("{path} is not a {what}"),
    }
}

/// Lists `$1` with one record per entry, NUL-terminated:
/// `kind TAB size mode mtime TAB link-target TAB name` (kind `d`, `f`, `l`
/// or `o`; the middle fields are empty without `stat`/`readlink`). Exits 2
/// when `$1` cannot be entered.
pub const LIST_SCRIPT: &str = r#"cd -- "$1" 2>/dev/null || exit 2
S=$(command -v stat); R=$(command -v readlink)
for f in * .[!.]* ..?*; do
  [ -e "$f" ] || [ -L "$f" ] || continue
  if [ -L "$f" ]; then t=l; elif [ -d "$f" ]; then t=d; elif [ -f "$f" ]; then t=f; else t=o; fi
  m=; [ -n "$S" ] && m=$("$S" -c '%s %a %Y' -- "$f" 2>/dev/null)
  l=; [ "$t" = l ] && [ -n "$R" ] && l=$("$R" -- "$f" 2>/dev/null)
  printf '%s\t%s\t%s\t%s\0' "$t" "$m" "$l" "$f"
done"#;

/// `sh -c LIST_SCRIPT sh <path>`.
#[must_use]
pub fn list_command(path: &str) -> Vec<String> {
    vec![
        "sh".into(),
        "-c".into(),
        LIST_SCRIPT.into(),
        "sh".into(),
        path.into(),
    ]
}

/// The records printed by [`LIST_SCRIPT`]; malformed ones are skipped.
#[must_use]
pub fn parse_listing(output: &[u8]) -> Vec<FileEntry> {
    output
        .split(|b| *b == 0)
        .filter_map(|record| {
            let record = String::from_utf8_lossy(record);
            let mut fields = record.splitn(4, '\t');
            let kind = match fields.next()?.trim_start_matches('\n') {
                "d" => EntryKind::Dir,
                "f" => EntryKind::File,
                "l" => EntryKind::Symlink,
                "o" => EntryKind::Other,
                _ => return None,
            };
            let meta: Vec<&str> = fields.next()?.split_whitespace().collect();
            let link = fields.next()?;
            let name = fields.next()?;
            if name.is_empty() || name.contains('/') {
                return None;
            }
            let number = |i: usize| meta.get(i).and_then(|v| v.parse::<i64>().ok());
            Some(FileEntry {
                name: name.to_owned(),
                kind,
                size: if kind == EntryKind::File {
                    number(0).and_then(|n| u64::try_from(n).ok()).unwrap_or(0)
                } else {
                    0
                },
                mode: meta
                    .get(1)
                    .and_then(|m| u32::from_str_radix(m, 8).ok())
                    .unwrap_or(0),
                mtime: number(2).unwrap_or(0),
                link: (kind == EntryKind::Symlink && !link.is_empty()).then(|| link.to_owned()),
            })
        })
        .collect()
}

fn sort_entries(entries: &mut [FileEntry]) {
    entries.sort_by(|a, b| {
        (a.kind != EntryKind::Dir)
            .cmp(&(b.kind != EntryKind::Dir))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

/// Lists the folder `path` (symlinked folders are followed): with `exec`
/// in a running container that has `sh`, else through the archive.
pub async fn list_dir(engine: &SharedEngine, id: &str, path: &str) -> EngineResult<DirListing> {
    let path = normalize(path);
    match engine.exec_capture(id, &list_command(&path)).await {
        Ok(out) if out.exit_code == Some(0) => {
            let mut entries = parse_listing(&out.stdout);
            let partial = entries.len() > LIST_ENTRY_LIMIT;
            entries.truncate(LIST_ENTRY_LIMIT);
            sort_entries(&mut entries);
            return Ok(DirListing {
                path,
                entries,
                partial,
            });
        }
        Ok(out) if out.exit_code == Some(2) => {
            return Err(EngineError::Api {
                status: 404,
                message: format!("{path} is not a folder, or cannot be opened"),
            });
        }
        // No shell (exit 126/127, or an exec error), or not running.
        Ok(out) => {
            tracing::debug!(code = ?out.exit_code, "exec listing failed: using the archive");
        }
        Err(error) if error.is_not_found() => return Err(error),
        Err(error) => tracing::debug!(%error, "exec listing failed: using the archive"),
    }
    list_dir_archive(engine, id, &path).await
}

/// Lists the folder `path` from its archive (see the module docs).
pub async fn list_dir_archive(
    engine: &SharedEngine,
    id: &str,
    path: &str,
) -> EngineResult<DirListing> {
    let path = normalize(path);
    // A trailing slash makes the engine follow a symlinked folder.
    let request = if path == "/" {
        path.clone()
    } else {
        format!("{path}/")
    };
    let mut stream = engine.archive(id, &request);
    let mut reader = TarReader::default();
    let mut root: Option<String> = None;
    let mut entries: Vec<FileEntry> = Vec::new();
    let mut not_dir = false;
    let mut read: u64 = 0;
    let mut partial = false;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        read += chunk.len() as u64;
        reader
            .push(&chunk, &mut |event| {
                let TarEvent::Entry(entry) = event else {
                    return;
                };
                let Some(root) = &root else {
                    not_dir = entry.kind != EntryKind::Dir;
                    root = Some(clean(&entry.name).to_owned());
                    return;
                };
                if let Some(name) = child_name(root, &entry.name) {
                    entries.push(FileEntry {
                        name,
                        kind: entry.kind,
                        size: entry.size,
                        mode: entry.mode,
                        mtime: entry.mtime,
                        link: entry.link,
                    });
                }
            })
            .map_err(EngineError::Transport)?;
        if not_dir {
            return Err(not_a("folder", &path));
        }
        if reader.finished {
            break;
        }
        if read >= LIST_BYTE_LIMIT || entries.len() >= LIST_ENTRY_LIMIT {
            // Dropping the stream cancels the rest of the download.
            partial = true;
            break;
        }
    }
    entries.truncate(LIST_ENTRY_LIMIT);
    sort_entries(&mut entries);
    Ok(DirListing {
        path,
        entries,
        partial,
    })
}

/// Streams the regular file `path`, giving its data to `sink` piece by
/// piece; stops after `limit` bytes. Returns (size, truncated).
async fn stream_file(
    engine: &SharedEngine,
    id: &str,
    path: &str,
    limit: u64,
    mut sink: impl FnMut(&[u8]) -> std::io::Result<()>,
) -> EngineResult<(u64, bool)> {
    let mut stream = engine.archive(id, path);
    let mut reader = TarReader::default();
    let mut first: Option<(EntryKind, u64, Option<String>)> = None;
    let mut entries = 0usize;
    let mut taken: u64 = 0;
    let mut write_error: Option<std::io::Error> = None;
    while let Some(chunk) = stream.next().await {
        let chunk = chunk?;
        reader
            .push(&chunk, &mut |event| match event {
                TarEvent::Entry(entry) => {
                    entries += 1;
                    if first.is_none() {
                        first = Some((entry.kind, entry.size, entry.link));
                    }
                }
                TarEvent::Data(data) if entries == 1 && write_error.is_none() => {
                    let room = usize::try_from(limit - taken).unwrap_or(usize::MAX);
                    let part = &data[..data.len().min(room)];
                    taken += part.len() as u64;
                    if let Err(e) = sink(part) {
                        write_error = Some(e);
                    }
                }
                TarEvent::Data(_) => {}
            })
            .map_err(EngineError::Transport)?;
        if let Some(error) = write_error {
            return Err(EngineError::Command(error.to_string()));
        }
        match &first {
            Some((EntryKind::Dir, ..)) => return Err(not_a("file", path)),
            Some((EntryKind::Symlink, _, link)) => {
                return Err(EngineError::Api {
                    status: 400,
                    message: format!("{path} is a link to {}", link.as_deref().unwrap_or("?")),
                });
            }
            Some((EntryKind::Other, ..)) => return Err(not_a("regular file", path)),
            _ => {}
        }
        if reader.finished || entries > 1 || taken >= limit {
            break;
        }
    }
    let size = first.map_or(0, |(_, size, _)| size);
    Ok((size, taken < size))
}

/// The first [`VIEW_LIMIT`] bytes of the file `path`, for the viewer.
pub async fn read_file(engine: &SharedEngine, id: &str, path: &str) -> EngineResult<FileContent> {
    let path = normalize(path);
    let mut data = Vec::new();
    let (size, truncated) = stream_file(engine, id, &path, VIEW_LIMIT, |part| {
        data.extend_from_slice(part);
        Ok(())
    })
    .await?;
    Ok(FileContent {
        path,
        size,
        data,
        truncated,
    })
}

/// Saves the file `path` of the container as `dest` on this machine.
/// Returns the bytes written.
pub async fn download_file(
    engine: &SharedEngine,
    id: &str,
    path: &str,
    dest: &Path,
) -> EngineResult<u64> {
    use std::io::Write;
    let path = normalize(path);
    let local = |e: std::io::Error| EngineError::Command(format!("{}: {e}", dest.display()));
    let mut file = std::io::BufWriter::new(std::fs::File::create(dest).map_err(local)?);
    let result = stream_file(engine, id, &path, u64::MAX, |part| file.write_all(part)).await;
    let flushed = file.flush().map_err(local);
    match result.and_then(|r| flushed.map(|()| r)) {
        Ok((size, _)) => Ok(size),
        Err(error) => {
            let _ = std::fs::remove_file(dest);
            Err(error)
        }
    }
}

/// Writes the local file `local` into the container's folder `dir`, keeping
/// its name and permissions (an existing file of that name is replaced).
/// Returns the path it now has in the container.
pub async fn upload_file(
    engine: &SharedEngine,
    id: &str,
    dir: &str,
    local: &Path,
) -> EngineResult<String> {
    let io = |e: std::io::Error| EngineError::Command(format!("{}: {e}", local.display()));
    let name = local
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .ok_or_else(|| EngineError::Command(format!("{} is not a file", local.display())))?;
    let meta = std::fs::metadata(local).map_err(io)?;
    if !meta.is_file() {
        return Err(EngineError::Command(format!(
            "{} is not a file",
            local.display()
        )));
    }
    if meta.len() > UPLOAD_LIMIT {
        return Err(EngineError::Command(format!(
            "{name} is larger than {}",
            crate::format::bytes(UPLOAD_LIMIT)
        )));
    }
    let data = std::fs::read(local).map_err(io)?;
    #[cfg(unix)]
    let mode = {
        use std::os::unix::fs::PermissionsExt;
        meta.permissions().mode() & 0o7777
    };
    #[cfg(not(unix))]
    let mode = 0o644;
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .and_then(|d| i64::try_from(d.as_secs()).ok())
        .unwrap_or(0);
    let dir = normalize(dir);
    let tar = crate::archive::single_file(&name, &data, mode, mtime);
    engine.put_archive(id, &dir, tar).await?;
    Ok(join(&dir, &name))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fake::FakeEngine;
    use std::sync::Arc;

    const SAMPLE: [(&str, EntryKind, &[u8]); 7] = [
        ("/etc", EntryKind::Dir, b""),
        ("/etc/hosts", EntryKind::File, b"127.0.0.1 localhost\n"),
        ("/etc/ssl", EntryKind::Dir, b""),
        ("/etc/ssl/cert.pem", EntryKind::File, b"PEM"),
        (
            "/etc/localtime",
            EntryKind::Symlink,
            b"/usr/share/zoneinfo/UTC",
        ),
        ("/bin", EntryKind::Dir, b""),
        ("/bin/app", EntryKind::File, b"\x7fELF\0\0"),
    ];

    /// The sample tree in the running `aaa111` and the stopped `bbb222`.
    fn engine() -> (Arc<FakeEngine>, SharedEngine) {
        let fake = Arc::new(FakeEngine::with_sample_data());
        fake.add_files("aaa111", &SAMPLE);
        fake.add_files("bbb222", &SAMPLE);
        let shared: SharedEngine = fake.clone();
        (fake, shared)
    }

    fn names(listing: &DirListing) -> Vec<&str> {
        listing.entries.iter().map(|e| e.name.as_str()).collect()
    }

    #[test]
    fn normalizes_paths() {
        assert_eq!(normalize(""), "/");
        assert_eq!(normalize("etc//ssl/./certs/"), "/etc/ssl/certs");
        assert_eq!(normalize("/a/b/../../.."), "/");
        assert_eq!(parent("/etc/ssl"), "/etc");
        assert_eq!(parent("/"), "/");
        assert_eq!(join("/etc", "hosts"), "/etc/hosts");
        assert_eq!(
            resolve("/etc", "/usr/share/zoneinfo/UTC"),
            "/usr/share/zoneinfo/UTC"
        );
        assert_eq!(resolve("/usr/lib", "../share/x"), "/usr/share/x");
        assert_eq!(base_name("/etc/hosts"), "hosts");
        assert_eq!(base_name("/"), "/");
        assert_eq!(child_name("etc", "etc/ssl/"), Some("ssl".into()));
        assert_eq!(child_name("etc", "etc/ssl/cert.pem"), None);
        assert_eq!(child_name("", "./bin/"), Some("bin".into()));
        assert_eq!(child_name("etc", "other/x"), None);
    }

    #[test]
    fn shows_permissions_like_ls() {
        let entry = |kind, mode| FileEntry {
            name: "x".into(),
            kind,
            size: 0,
            mode,
            mtime: 0,
            link: None,
        };
        assert_eq!(entry(EntryKind::Dir, 0o755).mode_string(), "drwxr-xr-x");
        assert_eq!(entry(EntryKind::File, 0o640).mode_string(), "-rw-r-----");
        assert_eq!(entry(EntryKind::Symlink, 0o777).mode_string(), "lrwxrwxrwx");
    }

    #[test]
    fn parses_listing_records() {
        let out = b"d\t4096 755 1700000000\t\tssl\0f\t20 644 1700000001\t\thosts\0\
l\t23 777 1700000002\t/usr/share/zoneinfo/UTC\tlocaltime\0f\t\t\tno stat\0\
f\t5 600 1\t\tname\twith tab\0x\tbad\0d\t1 755 1\t\ta/b\0";
        let entries = parse_listing(out);
        let names: Vec<_> = entries.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(
            names,
            ["ssl", "hosts", "localtime", "no stat", "name\twith tab"]
        );
        assert_eq!(entries[0].size, 0, "folders have no size");
        assert_eq!(
            (entries[1].size, entries[1].mode, entries[1].mtime),
            (20, 0o644, 1_700_000_001)
        );
        assert_eq!(entries[2].link.as_deref(), Some("/usr/share/zoneinfo/UTC"));
        assert_eq!((entries[3].size, entries[3].mode), (0, 0));
        assert_eq!(list_command("/etc")[4], "/etc");
    }

    #[tokio::test]
    async fn lists_a_running_container_with_one_exec() {
        let (fake, engine) = engine();
        let etc = list_dir(&engine, "aaa111", "/etc/").await;
        let etc = etc.unwrap_or_else(|e| panic!("list: {e}"));
        assert_eq!(names(&etc), ["ssl", "hosts", "localtime"], "folders first");
        assert_eq!(etc.path, "/etc");
        assert!(!etc.partial);
        assert_eq!(etc.entries[1].size, 20);
        assert_eq!(
            etc.entries[2].link.as_deref(),
            Some("/usr/share/zoneinfo/UTC")
        );
        assert_eq!(fake.state().execs, ["aaa111 sh"]);
        assert!(fake.state().archive_requests.is_empty());
        let file = list_dir(&engine, "aaa111", "/etc/hosts").await;
        assert!(file.is_err_and(|e| e.to_string().contains("not a folder")));
        let gone = list_dir(&engine, "zzz", "/").await;
        assert!(gone.is_err_and(|e| e.is_not_found()));
    }

    #[tokio::test]
    async fn falls_back_to_the_archive_without_a_shell_or_when_stopped() {
        let (fake, engine) = engine();
        let by_exec = list_dir(&engine, "aaa111", "/etc").await;
        fake.state().no_shell.push("aaa111".into());
        let distroless = list_dir(&engine, "aaa111", "/etc").await;
        let stopped = list_dir(&engine, "bbb222", "/etc").await;
        assert!(by_exec.is_ok());
        assert_eq!(by_exec, distroless);
        assert_eq!(by_exec, stopped);
        assert_eq!(
            fake.state().archive_requests,
            ["aaa111 /etc/", "bbb222 /etc/"]
        );
        let root = list_dir_archive(&engine, "bbb222", "/").await;
        assert_eq!(root.as_ref().map(names).ok(), Some(vec!["bin", "etc"]));
        let file = list_dir_archive(&engine, "bbb222", "/etc/hosts").await;
        assert!(file.is_err_and(|e| e.to_string().contains("not a folder")));
        let missing = list_dir(&engine, "bbb222", "/nope").await;
        assert!(missing.is_err_and(|e| e.is_not_found()));
    }

    #[tokio::test]
    async fn stops_reading_a_huge_archive() {
        let fake = Arc::new(FakeEngine::with_sample_data());
        let paths: Vec<String> = (0..=LIST_ENTRY_LIMIT)
            .map(|i| format!("/many/f{i:05}"))
            .collect();
        let mut files: Vec<(&str, EntryKind, &[u8])> = vec![("/many", EntryKind::Dir, b"")];
        files.extend(
            paths
                .iter()
                .map(|p| (p.as_str(), EntryKind::File, &b"x"[..])),
        );
        fake.add_files("bbb222", &files);
        let engine: SharedEngine = fake.clone();
        let listing = list_dir(&engine, "bbb222", "/many").await;
        let listing = listing.unwrap_or_else(|e| panic!("list: {e}"));
        assert!(listing.partial);
        assert_eq!(listing.entries.len(), LIST_ENTRY_LIMIT);
    }

    #[tokio::test]
    async fn reads_text_and_spots_binary_files() {
        let (_, engine) = engine();
        let hosts = read_file(&engine, "aaa111", "/etc/hosts").await;
        let hosts = hosts.unwrap_or_else(|e| panic!("read: {e}"));
        assert_eq!(hosts.text(), Some("127.0.0.1 localhost\n"));
        assert!(!hosts.truncated);
        let binary = read_file(&engine, "aaa111", "/bin/app").await;
        assert!(binary.is_ok_and(|f| f.text().is_none() && f.size == 6));
        let dir = read_file(&engine, "aaa111", "/etc").await;
        assert!(dir.is_err_and(|e| e.to_string().contains("not a file")));
        let link = read_file(&engine, "aaa111", "/etc/localtime").await;
        assert!(link.is_err_and(|e| e.to_string().contains("link to /usr/share")));
    }

    #[test]
    fn keeps_text_cut_mid_character() {
        let cut = FileContent {
            path: "/x".into(),
            size: 10,
            data: "héllo".as_bytes()[..2].to_vec(),
            truncated: true,
        };
        assert_eq!(cut.text(), Some("h"));
        let broken = FileContent {
            truncated: false,
            ..cut
        };
        assert_eq!(broken.text(), None);
    }

    #[tokio::test]
    async fn downloads_and_uploads_files() {
        let (fake, engine) = engine();
        let dir = tempfile::tempdir().unwrap_or_else(|e| panic!("{e}"));
        let dest = dir.path().join("hosts");
        let size = download_file(&engine, "aaa111", "/etc/hosts", &dest).await;
        assert_eq!(size.ok(), Some(20));
        assert_eq!(
            std::fs::read_to_string(&dest).unwrap_or_default(),
            "127.0.0.1 localhost\n"
        );
        let failed = download_file(&engine, "aaa111", "/etc", &dir.path().join("etc")).await;
        assert!(failed.is_err());
        assert!(!dir.path().join("etc").exists(), "no half-written file");

        let local = dir.path().join("app.env");
        std::fs::write(&local, "MODE=dev\n").unwrap_or_else(|e| panic!("{e}"));
        let uploaded = upload_file(&engine, "aaa111", "/etc/", &local).await;
        assert_eq!(uploaded.ok().as_deref(), Some("/etc/app.env"));
        assert!(
            fake.state()
                .calls
                .contains(&"upload aaa111 /etc/app.env".to_owned())
        );
        let back = read_file(&engine, "aaa111", "/etc/app.env").await;
        assert_eq!(
            back.ok()
                .and_then(|f| f.text().map(str::to_owned))
                .as_deref(),
            Some("MODE=dev\n")
        );
        let nowhere = upload_file(&engine, "aaa111", "/missing", &local).await;
        assert!(nowhere.is_err_and(|e| e.is_not_found()));
        let not_file = upload_file(&engine, "aaa111", "/etc", dir.path()).await;
        assert!(not_file.is_err());
    }
}

//! Installing language servers from the editor, into
//! `~/.ferrum/editor/servers/`: rust-analyzer through rustup (or its
//! GitHub release), npm packages (with Node.js downloaded from nodejs.org
//! when there is none), `go install` (with Go downloaded from go.dev when
//! there is none) and taplo's GitHub release. Toolchain downloads are
//! checked against their published SHA-256. Everything here blocks: run it
//! on a background thread. Progress goes to a log.

use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde_json::Value;
use sha2::{Digest, Sha256};

use super::servers::{ServerDef, find_program};

/// How a server can be installed, in order of preference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// `rustup component add <component>` (the rustup proxy then works).
    Rustup(&'static str),
    /// A gzipped binary; `{target}` is replaced per platform.
    GithubGz {
        url: &'static str,
        bin: &'static str,
        targets: Targets,
    },
    /// `npm install` of these packages into the server's folder.
    Npm(&'static [&'static str]),
    /// `go install <module>@latest` into the server's folder.
    Go(&'static str),
}

/// Platform names a GitHub release uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Targets {
    /// `aarch64-apple-darwin`, `x86_64-unknown-linux-gnu`, …
    RustTriple,
    /// `darwin-aarch64`, `linux-x86_64`, …
    OsArch,
}

pub fn methods(def: &ServerDef) -> &'static [Method] {
    match def.id {
        "rust" => &[
            Method::Rustup("rust-analyzer"),
            Method::GithubGz {
                url: "https://github.com/rust-lang/rust-analyzer/releases/latest/download/rust-analyzer-{target}.gz",
                bin: "rust-analyzer",
                targets: Targets::RustTriple,
            },
        ],
        "typescript" => &[Method::Npm(&["typescript", "typescript-language-server"])],
        "python" => &[Method::Npm(&["pyright"])],
        "json" | "css" | "html" => &[Method::Npm(&["vscode-langservers-extracted"])],
        "yaml" => &[Method::Npm(&["yaml-language-server"])],
        "shell" => &[Method::Npm(&["bash-language-server"])],
        "go" => &[Method::Go("golang.org/x/tools/gopls")],
        "toml" => &[Method::GithubGz {
            url: "https://github.com/tamasfe/taplo/releases/latest/download/taplo-{target}.gz",
            bin: "taplo",
            targets: Targets::OsArch,
        }],
        _ => &[],
    }
}

/// The folder a server installs into (npm and Go share their toolchains).
fn server_dir(servers: &Path, def: &ServerDef) -> PathBuf {
    match methods(def).first() {
        // One npm prefix for the three vscode-langservers-extracted servers.
        Some(Method::Npm(packages)) => servers.join(packages.last().copied().unwrap_or(def.id)),
        _ => servers.join(def.id),
    }
}

/// Folders with installed programs, to add to the server search path.
pub fn managed_bin_dirs(servers: &Path) -> Vec<PathBuf> {
    let mut dirs = vec![
        servers.join("node").join("bin"),
        servers.join("go").join("bin"),
    ];
    if let Ok(entries) = std::fs::read_dir(servers) {
        let mut found: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .flat_map(|p| [p.join("bin"), p.join("node_modules").join(".bin")])
            .filter(|p| p.is_dir())
            .collect();
        found.sort();
        dirs.extend(found);
    }
    dirs.retain(|d| d.is_dir());
    dirs.dedup();
    dirs
}

/// `path_env` with the installed programs after the user's own.
pub fn with_managed(path_env: &str, servers: &Path) -> String {
    let mut dirs: Vec<PathBuf> = std::env::split_paths(path_env).collect();
    for dir in managed_bin_dirs(servers) {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs)
        .map(|p| p.to_string_lossy().to_string())
        .unwrap_or_else(|_| path_env.to_string())
}

/// What an install will do, for the status bar ("rust-analyzer via rustup").
pub fn describe(def: &ServerDef, path_env: &str) -> String {
    let name = def.commands.first().map_or(def.id, |(n, _)| n);
    match pick(def, path_env) {
        Some(Method::Rustup(_)) => format!("{name} via rustup"),
        Some(Method::GithubGz { .. }) => format!("{name} from GitHub"),
        Some(Method::Npm(_)) if find_program("npm", path_env).is_some() => {
            format!("{name} via npm")
        }
        Some(Method::Npm(_)) => format!("Node.js and {name}"),
        Some(Method::Go(_)) if find_program("go", path_env).is_some() => {
            format!("{name} via go install")
        }
        Some(Method::Go(_)) => format!("Go and {name}"),
        None => name.to_string(),
    }
}

pub fn can_install(def: &ServerDef) -> bool {
    !methods(def).is_empty() && platform().is_some()
}

fn pick(def: &ServerDef, path_env: &str) -> Option<Method> {
    methods(def).iter().copied().find(|m| match m {
        Method::Rustup(_) => find_program("rustup", path_env).is_some(),
        _ => true,
    })
}

/// (os, arch) of this machine in the spelling download sites use.
fn platform() -> Option<(&'static str, &'static str)> {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        "linux" => "linux",
        _ => return None,
    };
    let arch = match std::env::consts::ARCH {
        "aarch64" => "aarch64",
        "x86_64" => "x86_64",
        _ => return None,
    };
    Some((os, arch))
}

fn github_target(targets: Targets) -> Option<String> {
    let (os, arch) = platform()?;
    Some(match (targets, os) {
        (Targets::RustTriple, "darwin") => format!("{arch}-apple-darwin"),
        (Targets::RustTriple, _) => format!("{arch}-unknown-linux-gnu"),
        (Targets::OsArch, _) => format!("{os}-{arch}"),
    })
}

/// Node.js and Go spell architectures differently.
fn node_arch(arch: &str) -> &'static str {
    if arch == "aarch64" { "arm64" } else { "x64" }
}

fn go_arch(arch: &str) -> &'static str {
    if arch == "aarch64" { "arm64" } else { "amd64" }
}

pub struct Installer<'a> {
    pub servers: PathBuf,
    pub path_env: String,
    pub log: &'a mut dyn Write,
}

impl Installer<'_> {
    fn say(&mut self, line: &str) {
        writeln!(self.log, "{line}").ok();
        self.log.flush().ok();
    }

    /// Installs `def`'s server. On success its program is on
    /// [`with_managed`]'s path (or the rustup proxy works).
    pub fn install(&mut self, def: &ServerDef) -> Result<(), String> {
        let method = pick(def, &self.path_env)
            .ok_or_else(|| format!("Ferrum Editor can't install the {} server", def.id))?;
        std::fs::create_dir_all(&self.servers).map_err(|e| e.to_string())?;
        self.say(&format!("Installing {}", describe(def, &self.path_env)));
        match method {
            Method::Rustup(component) => {
                let rustup = find_program("rustup", &self.path_env).ok_or("rustup not found")?;
                self.run(Command::new(rustup).args(["component", "add", component]))
            }
            Method::GithubGz { url, bin, targets } => {
                let target = github_target(targets).ok_or("unsupported platform")?;
                let url = url.replace("{target}", &target);
                let dir = server_dir(&self.servers, def).join("bin");
                std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
                let gz = dir.join(format!("{bin}.gz.download"));
                self.download(&url, &gz)?;
                let out = dir.join(bin);
                gunzip(&gz, &out)?;
                std::fs::remove_file(&gz).ok();
                make_executable(&out)?;
                self.say(&format!("Installed {}", out.display()));
                Ok(())
            }
            Method::Npm(packages) => {
                let npm = match find_program("npm", &self.path_env) {
                    Some(npm) => npm,
                    None => self.install_node()?,
                };
                let prefix = server_dir(&self.servers, def);
                std::fs::create_dir_all(&prefix).map_err(|e| e.to_string())?;
                let mut command = Command::new(npm);
                command
                    .args(["install", "--prefix"])
                    .arg(&prefix)
                    .args(["--no-audit", "--no-fund", "--loglevel=error"])
                    .args(packages.iter().map(|p| format!("{p}@latest")));
                self.run(&mut command)
            }
            Method::Go(module) => {
                let go = match find_program("go", &self.path_env) {
                    Some(go) => go,
                    None => self.install_go()?,
                };
                let bin = server_dir(&self.servers, def).join("bin");
                std::fs::create_dir_all(&bin).map_err(|e| e.to_string())?;
                let mut command = Command::new(go);
                command
                    .args(["install", &format!("{module}@latest")])
                    .env("GOBIN", &bin);
                self.run(&mut command)
            }
        }
    }

    /// Runs a command with the install PATH, logging its output.
    fn run(&mut self, command: &mut Command) -> Result<(), String> {
        let path = with_managed(&self.path_env, &self.servers);
        let output = command
            .env("PATH", &path)
            .stdin(Stdio::null())
            .output()
            .map_err(|e| format!("could not start {:?}: {e}", command.get_program()))?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        self.say(text.trim_end());
        if output.status.success() {
            Ok(())
        } else {
            let last = text
                .lines()
                .rev()
                .find(|l| !l.trim().is_empty())
                .unwrap_or("");
            Err(format!(
                "{} failed: {}",
                Path::new(command.get_program())
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().to_string()),
                last.trim()
            ))
        }
    }

    fn download(&mut self, url: &str, to: &Path) -> Result<(), String> {
        self.say(&format!("Downloading {url}"));
        let curl = find_program("curl", &self.path_env).ok_or("curl not found")?;
        self.run(
            Command::new(curl)
                .args(["-fsSL", "--retry", "2", "-o"])
                .arg(to)
                .arg(url),
        )
        .map_err(|e| format!("download failed ({url}): {e}"))
    }

    fn fetch_text(&mut self, url: &str) -> Result<String, String> {
        let file = self.servers.join(".download.txt");
        self.download(url, &file)?;
        let text = std::fs::read_to_string(&file).map_err(|e| e.to_string());
        std::fs::remove_file(&file).ok();
        text
    }

    /// Downloads the latest Node.js LTS into `servers/node`; returns npm.
    fn install_node(&mut self) -> Result<PathBuf, String> {
        let (os, arch) = platform().ok_or("unsupported platform")?;
        let index = self.fetch_text("https://nodejs.org/dist/index.json")?;
        let version = latest_node_lts(&index).ok_or("no Node.js LTS release found")?;
        let base = format!("node-{version}-{os}-{}", node_arch(arch));
        let file = format!("{base}.tar.gz");
        let sums = self.fetch_text(&format!("https://nodejs.org/dist/{version}/SHASUMS256.txt"))?;
        let expected = checksum_for(&sums, &file).ok_or("Node.js checksum not published")?;
        let archive = self.servers.join(&file);
        self.download(
            &format!("https://nodejs.org/dist/{version}/{file}"),
            &archive,
        )?;
        verify(&archive, &expected)?;
        self.say(&format!("Checksum OK; unpacking Node.js {version}"));
        let target = self.servers.join("node");
        extract_tar_gz(&archive, &self.servers, &base, &target)?;
        std::fs::remove_file(&archive).ok();
        Ok(target.join("bin").join("npm"))
    }

    /// Downloads the latest Go into `servers/go`; returns the go binary.
    fn install_go(&mut self) -> Result<PathBuf, String> {
        let (os, arch) = platform().ok_or("unsupported platform")?;
        let index = self.fetch_text("https://go.dev/dl/?mode=json")?;
        let (file, sha) =
            latest_go(&index, os, go_arch(arch)).ok_or("no Go release found for this platform")?;
        let archive = self.servers.join(&file);
        self.download(&format!("https://go.dev/dl/{file}"), &archive)?;
        verify(&archive, &sha)?;
        self.say(&format!("Checksum OK; unpacking {file}"));
        let target = self.servers.join("go");
        extract_tar_gz(&archive, &self.servers, "go", &target)?;
        std::fs::remove_file(&archive).ok();
        Ok(target.join("bin").join("go"))
    }
}

/// The newest LTS version in nodejs.org's `index.json` (newest first).
pub fn latest_node_lts(index: &str) -> Option<String> {
    let releases: Value = serde_json::from_str(index).ok()?;
    releases
        .as_array()?
        .iter()
        .find(|r| r["lts"].is_string())
        .and_then(|r| r["version"].as_str())
        .map(str::to_string)
}

/// The newest stable Go archive for `os` / `arch`: (file name, sha256).
pub fn latest_go(index: &str, os: &str, arch: &str) -> Option<(String, String)> {
    let releases: Value = serde_json::from_str(index).ok()?;
    let release = releases
        .as_array()?
        .iter()
        .find(|r| r["stable"].as_bool() == Some(true))?;
    let file = release["files"]
        .as_array()?
        .iter()
        .find(|f| f["os"] == os && f["arch"] == arch && f["kind"] == "archive")?;
    Some((
        file["filename"].as_str()?.to_string(),
        file["sha256"].as_str()?.to_string(),
    ))
}

/// The hash for `file` in a `SHASUMS256.txt`.
pub fn checksum_for(sums: &str, file: &str) -> Option<String> {
    sums.lines().find_map(|line| {
        let mut parts = line.split_whitespace();
        let hash = parts.next()?;
        (parts.next()? == file).then(|| hash.to_string())
    })
}

pub fn sha256_hex(path: &Path) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    let mut buf = vec![0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect())
}

fn verify(path: &Path, expected: &str) -> Result<(), String> {
    let actual = sha256_hex(path)?;
    if actual.eq_ignore_ascii_case(expected.trim()) {
        Ok(())
    } else {
        std::fs::remove_file(path).ok();
        Err(format!(
            "checksum mismatch for {}: the download was not used",
            path.file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().to_string())
        ))
    }
}

fn gunzip(from: &Path, to: &Path) -> Result<(), String> {
    let input = std::fs::File::open(from).map_err(|e| e.to_string())?;
    let mut decoder = flate2::read::GzDecoder::new(input);
    let partial = to.with_extension("partial");
    let mut output = std::fs::File::create(&partial).map_err(|e| e.to_string())?;
    std::io::copy(&mut decoder, &mut output).map_err(|e| format!("bad archive: {e}"))?;
    std::fs::rename(&partial, to).map_err(|e| e.to_string())
}

/// Unpacks `archive` (whose top folder is `top`) so that folder becomes
/// `target`, replacing an older copy only once the new one is complete.
fn extract_tar_gz(archive: &Path, work: &Path, top: &str, target: &Path) -> Result<(), String> {
    let staging = work.join(".unpack");
    std::fs::remove_dir_all(&staging).ok();
    std::fs::create_dir_all(&staging).map_err(|e| e.to_string())?;
    let file = std::fs::File::open(archive).map_err(|e| e.to_string())?;
    tar::Archive::new(flate2::read::GzDecoder::new(file))
        .unpack(&staging)
        .map_err(|e| format!("could not unpack: {e}"))?;
    let unpacked = staging.join(top);
    if !unpacked.is_dir() {
        return Err(format!("unexpected archive layout (no {top}/)"));
    }
    std::fs::remove_dir_all(target).ok();
    std::fs::rename(&unpacked, target).map_err(|e| e.to_string())?;
    std::fs::remove_dir_all(&staging).ok();
    Ok(())
}

fn make_executable(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    let _ = path;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lsp::servers::{server_by_id, with_project_bins};

    fn script(dir: &Path, name: &str, body: &str) {
        let path = dir.join(name);
        std::fs::write(&path, format!("#!/bin/sh\n{body}\n")).expect("write");
        make_executable(&path).expect("chmod");
    }

    #[test]
    fn every_known_server_can_be_installed_here() {
        for def in crate::lsp::servers::SERVERS {
            assert!(!methods(def).is_empty(), "{}", def.id);
        }
        assert!(can_install(server_by_id("rust").expect("rust")));
    }

    #[test]
    fn rust_prefers_rustup_when_present() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_string_lossy().to_string();
        let rust = server_by_id("rust").expect("rust");
        assert!(matches!(pick(rust, &path), Some(Method::GithubGz { .. })));
        assert_eq!(describe(rust, &path), "rust-analyzer from GitHub");
        script(dir.path(), "rustup", "exit 0");
        assert_eq!(pick(rust, &path), Some(Method::Rustup("rust-analyzer")));
        assert_eq!(describe(rust, &path), "rust-analyzer via rustup");
        let go = server_by_id("go").expect("go");
        assert_eq!(describe(go, &path), "Go and gopls");
    }

    #[test]
    fn npm_installs_into_the_servers_folder_and_is_found_afterwards() {
        let tools = tempfile::tempdir().expect("tempdir");
        let servers = tempfile::tempdir().expect("tempdir");
        // A stand-in npm that creates the package's bin like npm does.
        script(
            tools.path(),
            "npm",
            r#"prefix="$3"; mkdir -p "$prefix/node_modules/.bin"; printf '#!/bin/sh\n' > "$prefix/node_modules/.bin/typescript-language-server"; chmod +x "$prefix/node_modules/.bin/typescript-language-server"; echo "added 2 packages""#,
        );
        // Commands run with this PATH: the tools plus the system's.
        let path = format!("{}:/bin:/usr/bin", tools.path().display());
        let mut log = Vec::new();
        let ts = server_by_id("typescript").expect("ts");
        Installer {
            servers: servers.path().to_path_buf(),
            path_env: path.clone(),
            log: &mut log,
        }
        .install(ts)
        .expect("install");
        let log = String::from_utf8(log).expect("utf8");
        assert!(
            log.contains("via npm") && log.contains("added 2 packages"),
            "{log}"
        );
        let full = with_managed(&path, servers.path());
        let resolved =
            crate::lsp::servers::resolve(ts, &crate::config::LspConfig::default(), &full);
        assert!(
            matches!(resolved, crate::lsp::servers::Resolved::Found { ref program, .. }
                if program.starts_with(servers.path())),
            "{resolved:?}"
        );
        // Project bins still come first.
        assert!(
            with_project_bins(&full, servers.path(), &servers.path().join("x.ts"))
                .contains("node_modules")
        );
    }

    #[test]
    fn failures_report_the_last_line() {
        let tools = tempfile::tempdir().expect("tempdir");
        let servers = tempfile::tempdir().expect("tempdir");
        script(
            tools.path(),
            "npm",
            "echo 'npm ERR! network unreachable' >&2; exit 1",
        );
        let mut log = Vec::new();
        let error = Installer {
            servers: servers.path().to_path_buf(),
            path_env: tools.path().to_string_lossy().to_string(),
            log: &mut log,
        }
        .install(server_by_id("yaml").expect("yaml"))
        .expect_err("fails");
        assert_eq!(error, "npm failed: npm ERR! network unreachable");
    }

    #[test]
    fn toolchain_indexes_and_checksums() {
        let node = r#"[{"version":"v27.0.0","lts":false},{"version":"v26.13.0","lts":"Lithium"},{"version":"v24.1.0","lts":"Krypton"}]"#;
        assert_eq!(latest_node_lts(node).as_deref(), Some("v26.13.0"));
        let go = r#"[{"version":"go1.28rc1","stable":false,"files":[]},
            {"version":"go1.27.4","stable":true,"files":[
              {"filename":"go1.27.4.darwin-arm64.tar.gz","os":"darwin","arch":"arm64","kind":"archive","sha256":"abc"},
              {"filename":"go1.27.4.darwin-arm64.pkg","os":"darwin","arch":"arm64","kind":"installer","sha256":"def"}]}]"#;
        assert_eq!(
            latest_go(go, "darwin", "arm64"),
            Some(("go1.27.4.darwin-arm64.tar.gz".into(), "abc".into()))
        );
        assert_eq!(latest_go(go, "linux", "amd64"), None);
        let sums = "111  node-v26.13.0-darwin-arm64.tar.gz\n222  node-v26.13.0-linux-x64.tar.xz\n";
        assert_eq!(
            checksum_for(sums, "node-v26.13.0-darwin-arm64.tar.gz").as_deref(),
            Some("111")
        );
        assert_eq!(checksum_for(sums, "nope.tar.gz"), None);
    }

    #[test]
    fn verifies_hashes_and_unpacks_archives() {
        let dir = tempfile::tempdir().expect("tempdir");
        // Build node-x/bin/npm as a .tar.gz.
        let archive = dir.path().join("node-x.tar.gz");
        {
            let file = std::fs::File::create(&archive).expect("create");
            let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
            let mut builder = tar::Builder::new(gz);
            let data = b"#!/bin/sh\n";
            let mut header = tar::Header::new_gnu();
            header.set_size(data.len() as u64);
            header.set_mode(0o755);
            header.set_cksum();
            builder
                .append_data(&mut header, "node-x/bin/npm", &data[..])
                .expect("append");
            builder.into_inner().expect("gz").finish().expect("finish");
        }
        let hash = sha256_hex(&archive).expect("hash");
        assert_eq!(hash.len(), 64);
        assert!(verify(&archive, "00").is_err());
        assert!(!archive.exists(), "a bad download is deleted");
        // Again, verified this time.
        let file = std::fs::File::create(&archive).expect("create");
        let gz = flate2::write::GzEncoder::new(file, flate2::Compression::fast());
        let mut builder = tar::Builder::new(gz);
        let mut header = tar::Header::new_gnu();
        header.set_size(10);
        header.set_mode(0o755);
        header.set_cksum();
        builder
            .append_data(&mut header, "node-x/bin/npm", &b"#!/bin/sh\n"[..])
            .expect("append");
        builder.into_inner().expect("gz").finish().expect("finish");
        let hash = sha256_hex(&archive).expect("hash");
        verify(&archive, &hash.to_uppercase()).expect("verified");
        let target = dir.path().join("node");
        extract_tar_gz(&archive, dir.path(), "node-x", &target).expect("unpack");
        assert!(target.join("bin/npm").is_file());
        assert!(managed_bin_dirs(dir.path()).contains(&target.join("bin")));
    }
}

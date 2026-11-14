//! The VM an engine runs in on macOS — Colima, OrbStack, Docker Desktop or a
//! Podman machine: which are installed, whether one is running, and starting
//! or stopping it through its own CLI. Also the idle auto-stop setting,
//! shared by the app and `fec` (`~/.ferrum/ferrumdock/vm.json`).

use crate::endpoint::{Candidate, Endpoint};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Vm {
    Colima,
    OrbStack,
    #[serde(rename = "desktop")]
    DockerDesktop,
    Podman,
}

/// Install locations searched after `PATH` (apps opened from Finder get a
/// minimal one), relative to `$HOME` when not absolute.
const BIN_DIRS: [&str; 5] = [
    "/opt/homebrew/bin",
    "/usr/local/bin",
    ".orbstack/bin",
    "/Applications/OrbStack.app/Contents/MacOS/xbin",
    "/Applications/Docker.app/Contents/Resources/bin",
];

impl Vm {
    pub const ALL: [Vm; 4] = [Vm::Colima, Vm::OrbStack, Vm::DockerDesktop, Vm::Podman];

    /// The name `fec --vm` and `vm.json` use.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Self::Colima => "colima",
            Self::OrbStack => "orbstack",
            Self::DockerDesktop => "desktop",
            Self::Podman => "podman",
        }
    }

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Self::Colima => "Colima",
            Self::OrbStack => "OrbStack",
            Self::DockerDesktop => "Docker Desktop",
            Self::Podman => "Podman",
        }
    }

    #[must_use]
    pub fn parse(key: &str) -> Option<Self> {
        match key.to_ascii_lowercase().as_str() {
            "colima" => Some(Self::Colima),
            "orbstack" | "orb" => Some(Self::OrbStack),
            "desktop" | "docker-desktop" | "docker" => Some(Self::DockerDesktop),
            "podman" => Some(Self::Podman),
            _ => None,
        }
    }

    /// The VM behind an engine the app connected to, by its socket.
    #[must_use]
    pub fn for_candidate(candidate: &Candidate) -> Option<Self> {
        match candidate.source {
            "Colima" => return Some(Self::Colima),
            "OrbStack" => return Some(Self::OrbStack),
            "Docker Desktop" => return Some(Self::DockerDesktop),
            _ => {}
        }
        let Endpoint::Unix(path) = &candidate.endpoint else {
            return None;
        };
        let path = path.to_string_lossy();
        if path.contains("/.colima/") {
            Some(Self::Colima)
        } else if path.contains("/.orbstack/") {
            Some(Self::OrbStack)
        } else if path.contains("/.docker/run/") {
            Some(Self::DockerDesktop)
        } else if path.contains("podman") {
            Some(Self::Podman)
        } else {
            None
        }
    }

    /// The program that controls it.
    fn program(self) -> &'static str {
        match self {
            Self::Colima => "colima",
            Self::OrbStack => "orb",
            Self::DockerDesktop => "docker",
            Self::Podman => "podman",
        }
    }

    /// Whether it is installed on this machine.
    #[must_use]
    pub fn installed(self) -> bool {
        match self {
            Self::DockerDesktop => Path::new("/Applications/Docker.app").exists(),
            Self::OrbStack => {
                Path::new("/Applications/OrbStack.app").exists() || find_program("orb").is_some()
            }
            other => find_program(other.program()).is_some(),
        }
    }

    /// The Docker socket it serves, where that is fixed.
    #[must_use]
    pub fn socket(self, home: &Path) -> Option<PathBuf> {
        match self {
            Self::Colima => Some(home.join(".colima/default/docker.sock")),
            Self::OrbStack => Some(home.join(".orbstack/run/docker.sock")),
            Self::DockerDesktop => Some(home.join(".docker/run/docker.sock")),
            Self::Podman => None,
        }
    }

    /// Whether it is running: its socket answers (Podman: the machine says so).
    #[must_use]
    pub fn is_running(self) -> bool {
        if self == Self::Podman {
            return run_output(self, &["machine", "inspect", "--format", "{{.State}}"])
                .is_ok_and(|out| String::from_utf8_lossy(&out.stdout).contains("running"));
        }
        home()
            .and_then(|home| self.socket(&home))
            .is_some_and(|socket| socket_answers(&socket))
    }

    /// Starts it, passing `extra` to its CLI (e.g. `--cpu 4` for Colima).
    pub fn start(self, extra: &[String]) -> Result<(), String> {
        let mut args: Vec<String> = match self {
            Self::Colima | Self::OrbStack => vec!["start".into()],
            Self::DockerDesktop => vec!["desktop".into(), "start".into()],
            Self::Podman => vec!["machine".into(), "start".into()],
        };
        args.extend(extra.iter().cloned());
        let result = run(self, &args);
        if result.is_err() && self == Self::DockerDesktop {
            // Docker Desktop before 4.37 has no `docker desktop`: open the app.
            return status(Command::new("/usr/bin/open").args(["-a", "Docker"]));
        }
        result
    }

    /// Stops it (the containers in it stop too).
    pub fn stop(self, extra: &[String]) -> Result<(), String> {
        let mut args: Vec<String> = match self {
            Self::Colima | Self::OrbStack => vec!["stop".into()],
            Self::DockerDesktop => vec!["desktop".into(), "stop".into()],
            Self::Podman => vec!["machine".into(), "stop".into()],
        };
        args.extend(extra.iter().cloned());
        let result = run(self, &args);
        if result.is_err() && self == Self::DockerDesktop {
            return status(Command::new("/usr/bin/osascript").args(["-e", "quit app \"Docker\""]));
        }
        result
    }
}

fn home() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// Finds `name` on `PATH`, then in the usual install locations.
#[must_use]
pub fn find_program(name: &str) -> Option<PathBuf> {
    let home = home();
    std::env::var_os("PATH")
        .into_iter()
        .flat_map(|p| std::env::split_paths(&p).collect::<Vec<_>>())
        .chain(BIN_DIRS.iter().filter_map(|d| {
            if d.starts_with('/') {
                Some(PathBuf::from(d))
            } else {
                home.as_ref().map(|h| h.join(d))
            }
        }))
        .map(|dir| dir.join(name))
        .find(|p| p.is_file())
}

fn socket_answers(path: &Path) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(path).is_ok()
    }
    #[cfg(not(unix))]
    {
        path.exists()
    }
}

/// `PATH` for the VM's CLI: ours plus the install locations (Colima needs
/// `limactl` and `docker` from Homebrew).
fn child_path() -> std::ffi::OsString {
    let mut dirs: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|p| std::env::split_paths(&p).collect())
        .unwrap_or_default();
    for dir in ["/opt/homebrew/bin", "/usr/local/bin"] {
        let dir = PathBuf::from(dir);
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    std::env::join_paths(dirs).unwrap_or_default()
}

fn command(vm: Vm) -> Result<Command, String> {
    let program = find_program(vm.program()).ok_or_else(|| {
        format!(
            "{} is not installed (no `{}` found)",
            vm.name(),
            vm.program()
        )
    })?;
    let mut command = Command::new(program);
    command.env("PATH", child_path());
    Ok(command)
}

fn run_output(vm: Vm, args: &[&str]) -> Result<Output, String> {
    command(vm)?
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())
}

fn run(vm: Vm, args: &[String]) -> Result<(), String> {
    let output = command(vm)?
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Err(stderr
            .lines()
            .rev()
            .find(|l| !l.trim().is_empty())
            .unwrap_or("failed")
            .trim()
            .to_owned())
    }
}

fn status(command: &mut Command) -> Result<(), String> {
    match command.status() {
        Ok(s) if s.success() => Ok(()),
        Ok(s) => Err(format!("exited with {s}")),
        Err(e) => Err(e.to_string()),
    }
}

/// The installed VMs, in [`Vm::ALL`] order.
#[must_use]
pub fn installed() -> Vec<Vm> {
    Vm::ALL.into_iter().filter(|vm| vm.installed()).collect()
}

/// The VM to act on: the chosen one, else one that is running, else the
/// first installed.
#[must_use]
pub fn pick(chosen: Option<Vm>) -> Option<Vm> {
    if chosen.is_some() {
        return chosen;
    }
    let installed = installed();
    installed
        .iter()
        .copied()
        .find(|vm| vm.is_running())
        .or_else(|| installed.first().copied())
}

/// The VM settings shared by the app and `fec`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct VmSettings {
    /// The VM `fec start` / `stop` use; `None` picks one.
    pub vm: Option<Vm>,
    /// Stop the VM after this many minutes with no running container
    /// (while FerrumContainers is open); 0 never stops it.
    pub auto_stop_minutes: u32,
}

impl Default for VmSettings {
    fn default() -> Self {
        Self {
            vm: None,
            auto_stop_minutes: 30,
        }
    }
}

impl VmSettings {
    /// `~/.ferrum/ferrumdock/vm.json` (or under `$FERRUM_HOME`).
    #[must_use]
    pub fn file() -> Option<PathBuf> {
        let root = std::env::var_os("FERRUM_HOME")
            .map(PathBuf::from)
            .or_else(|| home().map(|h| h.join(".ferrum")))?;
        Some(root.join("ferrumdock").join("vm.json"))
    }

    #[must_use]
    pub fn load() -> Self {
        Self::file()
            .and_then(|f| std::fs::read(f).ok())
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) -> Result<(), String> {
        let file = Self::file().ok_or("no home folder")?;
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(file, json).map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn knows_the_vm_behind_a_socket() {
        let c = |path: &str, source| Candidate::new(Endpoint::Unix(path.into()), source);
        assert_eq!(
            Vm::for_candidate(&c("/h/.colima/default/docker.sock", "Colima")),
            Some(Vm::Colima)
        );
        assert_eq!(
            Vm::for_candidate(&c("/h/.colima/dev/docker.sock", "Docker context")),
            Some(Vm::Colima)
        );
        assert_eq!(
            Vm::for_candidate(&c("/h/.docker/run/docker.sock", "Docker context")),
            Some(Vm::DockerDesktop)
        );
        assert_eq!(
            Vm::for_candidate(&c("/var/run/docker.sock", "Docker")),
            None
        );
    }

    #[test]
    fn keys_round_trip() {
        for vm in Vm::ALL {
            assert_eq!(Vm::parse(vm.key()), Some(vm));
        }
        assert_eq!(Vm::parse("orb"), Some(Vm::OrbStack));
        let json = serde_json::to_string(&VmSettings::default()).expect("json");
        assert_eq!(json, r#"{"vm":null,"auto_stop_minutes":30}"#);
        let s: VmSettings = serde_json::from_str(r#"{"vm":"desktop"}"#).expect("parse");
        assert_eq!((s.vm, s.auto_stop_minutes), (Some(Vm::DockerDesktop), 30));
    }
}

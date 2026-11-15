//! The project an app is working on, shared by every Ferrum app.
//!
//! Recently opened projects live in `~/.ferrum/projects.json`, so a folder
//! opened in FerrumIDE shows up in FerrumGIT's and FerrumTerm's
//! switchers too. This module is the data side (no UI): the list, the git
//! branch, `~`-shortened paths and opening a folder in another Ferrum app.
//! [`crate::ProjectBar`] draws it.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// How many projects the list keeps.
pub const MAX_RECENT: usize = 50;

/// One entry of `~/.ferrum/projects.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecentProject {
    pub path: PathBuf,
    /// Unix seconds of the last time any app opened it.
    #[serde(default)]
    pub opened_at: u64,
    /// Binary names of the apps that opened it (`ferrumeditor`, …).
    #[serde(default)]
    pub apps: Vec<String>,
}

impl RecentProject {
    /// The folder name, e.g. `ferrum`.
    #[must_use]
    pub fn name(&self) -> String {
        project_name(&self.path)
    }
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct ProjectsFile {
    #[serde(default)]
    projects: Vec<RecentProject>,
}

/// `~/.ferrum/projects.json`.
#[must_use]
pub fn projects_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(Path::new(&home).join(".ferrum").join("projects.json"))
}

/// The folder name of `path` (the path itself for `/`).
#[must_use]
pub fn project_name(path: &Path) -> String {
    path.file_name().map_or_else(
        || path.display().to_string(),
        |name| name.to_string_lossy().into_owned(),
    )
}

/// Recent projects in `file`, newest first. A missing or unreadable file
/// is an empty list.
#[must_use]
pub fn load(file: &Path) -> Vec<RecentProject> {
    std::fs::read_to_string(file)
        .ok()
        .and_then(|text| serde_json::from_str::<ProjectsFile>(&text).ok())
        .map(|f| f.projects)
        .unwrap_or_default()
}

fn save(file: &Path, projects: Vec<RecentProject>) -> std::io::Result<()> {
    if let Some(parent) = file.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let text =
        serde_json::to_string_pretty(&ProjectsFile { projects }).map_err(std::io::Error::other)?;
    // Write then rename, so an app reading at the same moment never sees
    // half a file.
    let tmp = file.with_extension(format!("json.{}.tmp", std::process::id()));
    std::fs::write(&tmp, text)?;
    std::fs::rename(&tmp, file)
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Moves `path` to the front of `file`'s list, noting that `app` opened
/// it, and returns the new list.
pub fn record(file: &Path, path: &Path, app: &str) -> std::io::Result<Vec<RecentProject>> {
    let mut projects = load(file);
    let mut entry = match projects.iter().position(|p| p.path == path) {
        Some(index) => projects.remove(index),
        None => RecentProject {
            path: path.to_path_buf(),
            opened_at: 0,
            apps: Vec::new(),
        },
    };
    entry.opened_at = now();
    if !app.is_empty() && !entry.apps.iter().any(|a| a == app) {
        entry.apps.push(app.to_string());
    }
    projects.insert(0, entry);
    projects.truncate(MAX_RECENT);
    save(file, projects.clone())?;
    Ok(projects)
}

/// Removes `path` from `file`'s list and returns the new list.
pub fn forget(file: &Path, path: &Path) -> std::io::Result<Vec<RecentProject>> {
    let mut projects = load(file);
    projects.retain(|p| p.path != path);
    save(file, projects.clone())?;
    Ok(projects)
}

/// `path` with the home folder written as `~`.
#[must_use]
pub fn display_path(path: &Path, home: Option<&Path>) -> String {
    match home.map(|home| path.strip_prefix(home)) {
        Some(Ok(rest)) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(Ok(rest)) => format!("~/{}", rest.display()),
        _ => path.display().to_string(),
    }
}

/// The user's home folder.
#[must_use]
pub fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME").map(PathBuf::from)
}

/// The top of the git repository containing `path`, if any.
#[must_use]
pub fn git_root(path: &Path) -> Option<PathBuf> {
    path.ancestors()
        .find(|dir| dir.join(".git").exists())
        .map(Path::to_path_buf)
}

/// The project a folder belongs to: its git repository, or the folder.
#[must_use]
pub fn project_root(path: &Path) -> PathBuf {
    git_root(path).unwrap_or_else(|| path.to_path_buf())
}

/// The checked-out branch of the repository containing `path` (the short
/// commit id when detached). Reads `.git/HEAD` directly: cheap enough to
/// call on every switch.
#[must_use]
pub fn git_branch(path: &Path) -> Option<String> {
    let root = git_root(path)?;
    let dot_git = root.join(".git");
    // Worktrees and submodules have a `.git` file: `gitdir: <path>`.
    let git_dir = if dot_git.is_file() {
        let text = std::fs::read_to_string(&dot_git).ok()?;
        let dir = PathBuf::from(text.trim().strip_prefix("gitdir:")?.trim());
        if dir.is_absolute() {
            dir
        } else {
            root.join(dir)
        }
    } else {
        dot_git
    };
    let head = std::fs::read_to_string(git_dir.join("HEAD")).ok()?;
    let head = head.trim();
    match head.strip_prefix("ref:") {
        Some(reference) => {
            let reference = reference.trim();
            Some(
                reference
                    .strip_prefix("refs/heads/")
                    .unwrap_or(reference)
                    .to_string(),
            )
        }
        None if head.len() >= 7 => Some(head[..7].to_string()),
        None => None,
    }
}

/// Ferrum apps a project can be opened in from the project bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FerrumApp {
    Editor,
    Git,
    Term,
}

impl FerrumApp {
    pub const ALL: [FerrumApp; 3] = [FerrumApp::Editor, FerrumApp::Git, FerrumApp::Term];

    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            FerrumApp::Editor => "FerrumIDE",
            FerrumApp::Git => "FerrumGIT",
            FerrumApp::Term => "FerrumTerm",
        }
    }

    /// Short button label.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            FerrumApp::Editor => "IDE",
            FerrumApp::Git => "Git",
            FerrumApp::Term => "Term",
        }
    }

    /// The executable's file name (also its link in `~/.ferrum/bin`).
    #[must_use]
    pub fn binary(self) -> &'static str {
        match self {
            FerrumApp::Editor => "ferrumeditor",
            FerrumApp::Git => "ferrumgit",
            FerrumApp::Term => "ferrumterm",
        }
    }

    /// Its `.app` bundle names, the current one first. The launcher renames
    /// its copy when an app is renamed; one installed by hand may keep the
    /// old name (FerrumIDE was Ferrum Editor).
    #[must_use]
    pub fn bundle_names(self) -> &'static [&'static str] {
        match self {
            FerrumApp::Editor => &["FerrumIDE", "Ferrum Editor"],
            FerrumApp::Git => &["FerrumGIT"],
            FerrumApp::Term => &["FerrumTerm"],
        }
    }

    /// The app a binary name belongs to.
    #[must_use]
    pub fn from_binary(binary: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|app| app.binary() == binary)
    }
}

/// Where `binary` is: the launcher's link in `~/.ferrum/bin`, else next
/// to the running executable (development builds, app bundles).
#[must_use]
pub fn find_binary(binary: &str, home: Option<&Path>) -> Option<PathBuf> {
    let linked = home.map(|home| home.join(".ferrum").join("bin").join(binary));
    let sibling = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join(binary)));
    linked
        .into_iter()
        .chain(sibling)
        .find(|candidate| candidate.is_file())
}

/// Where `app`'s executable is: [`find_binary`], else inside its `.app`
/// under any of its [bundle names](FerrumApp::bundle_names) in
/// `~/Applications/Ferrum`, `~/Applications`, `/Applications/Ferrum` or
/// `/Applications`.
#[must_use]
pub fn find_app(app: FerrumApp, home: Option<&Path>) -> Option<PathBuf> {
    find_binary(app.binary(), home).or_else(|| {
        let mut dirs: Vec<PathBuf> = Vec::new();
        if let Some(home) = home {
            dirs.push(home.join("Applications/Ferrum"));
            dirs.push(home.join("Applications"));
        }
        dirs.push(PathBuf::from("/Applications/Ferrum"));
        dirs.push(PathBuf::from("/Applications"));
        dirs.iter()
            .flat_map(|dir| {
                app.bundle_names().iter().map(move |name| {
                    dir.join(format!("{name}.app"))
                        .join("Contents/MacOS")
                        .join(app.binary())
                })
            })
            .find(|candidate| candidate.is_file())
    })
}

/// Opens `folder` in `app`, in the background, like the launcher's shell
/// commands (`fei .`, `feg .`, `fet`).
pub fn open_in(app: FerrumApp, folder: &Path) -> Result<(), String> {
    let home = home_dir();
    let Some(exe) = find_app(app, home.as_deref()) else {
        return Err(format!(
            "{} is not installed. Install it in FerrumLauncher.",
            app.name()
        ));
    };
    let spawn = |command: &mut Command| {
        command
            .current_dir(folder)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .map(drop)
            .map_err(|e| format!("Could not start {}: {e}", app.name()))
    };
    if app == FerrumApp::Term {
        // A session in the folder first, which the window then shows.
        let real = std::fs::canonicalize(&exe).unwrap_or_else(|_| exe.clone());
        if let Some(fterm) = real.parent().map(|dir| dir.join("fterm"))
            && fterm.is_file()
        {
            Command::new(fterm)
                .args(["new", &project_name(folder), "-c"])
                .arg(folder)
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .ok();
        }
        return spawn(&mut Command::new(&exe));
    }
    spawn(Command::new(&exe).arg(folder))
}

/// The Editor's command-line form of `file` at a 1-based `line`
/// (`src/main.rs:42`).
#[must_use]
pub fn editor_target(file: &Path, line: Option<u32>) -> std::ffi::OsString {
    let mut target = file.as_os_str().to_owned();
    if let Some(line) = line {
        target.push(format!(":{line}"));
    }
    target
}

/// Opens `file` in FerrumIDE (at a 1-based `line`), with `project` as
/// its folder: `ferrumeditor <project> <file>:<line>`.
pub fn open_file_in_editor(
    project: Option<&Path>,
    file: &Path,
    line: Option<u32>,
) -> Result<(), String> {
    let app = FerrumApp::Editor;
    let Some(exe) = find_app(app, home_dir().as_deref()) else {
        return Err(format!(
            "{} is not installed. Install it in FerrumLauncher.",
            app.name()
        ));
    };
    let mut command = Command::new(&exe);
    if let Some(project) = project {
        command.current_dir(project).arg(project);
    }
    command
        .arg(editor_target(file, line))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(drop)
        .map_err(|e| format!("Could not start {}: {e}", app.name()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn record_moves_to_front_and_remembers_apps() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("projects.json");
        assert!(load(&file).is_empty());
        record(&file, Path::new("/p/a"), "ferrumeditor").expect("a");
        record(&file, Path::new("/p/b"), "ferrumgit").expect("b");
        let list = record(&file, Path::new("/p/a"), "ferrumgit").expect("a again");
        assert_eq!(list.len(), 2);
        assert_eq!(list[0].path, Path::new("/p/a"));
        assert_eq!(list[0].apps, ["ferrumeditor", "ferrumgit"]);
        assert_eq!(list[0].name(), "a");
        assert_eq!(load(&file), list);
        let list = forget(&file, Path::new("/p/a")).expect("forget");
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].path, Path::new("/p/b"));
    }

    #[test]
    fn list_is_capped_and_bad_files_are_empty() {
        let dir = tempfile::tempdir().expect("tempdir");
        let file = dir.path().join("projects.json");
        for i in 0..MAX_RECENT + 5 {
            record(&file, &PathBuf::from(format!("/p/{i}")), "").expect("record");
        }
        let list = load(&file);
        assert_eq!(list.len(), MAX_RECENT);
        assert_eq!(
            list[0].path,
            PathBuf::from(format!("/p/{}", MAX_RECENT + 4))
        );
        std::fs::write(&file, "not json").expect("write");
        assert!(load(&file).is_empty());
    }

    #[test]
    fn paths_are_shown_from_home() {
        let home = Path::new("/Users/me");
        assert_eq!(
            display_path(Path::new("/Users/me/code/x"), Some(home)),
            "~/code/x"
        );
        assert_eq!(display_path(home, Some(home)), "~");
        assert_eq!(display_path(Path::new("/opt/x"), Some(home)), "/opt/x");
        assert_eq!(display_path(Path::new("/opt/x"), None), "/opt/x");
    }

    #[test]
    fn branches_are_read_from_head() {
        let dir = tempfile::tempdir().expect("tempdir");
        let repo = dir.path().join("repo");
        std::fs::create_dir_all(repo.join(".git")).expect("mkdir");
        std::fs::create_dir_all(repo.join("src/deep")).expect("mkdir");
        std::fs::write(repo.join(".git/HEAD"), "ref: refs/heads/feature/x\n").expect("head");
        assert_eq!(
            git_branch(&repo.join("src/deep")).as_deref(),
            Some("feature/x")
        );
        assert_eq!(project_root(&repo.join("src/deep")), repo);
        std::fs::write(repo.join(".git/HEAD"), "0123456789abcdef\n").expect("head");
        assert_eq!(git_branch(&repo).as_deref(), Some("0123456"));
        // A worktree: `.git` is a file pointing at the real git dir.
        let wt = dir.path().join("wt");
        let real = dir.path().join("real-git");
        std::fs::create_dir_all(&wt).expect("mkdir");
        std::fs::create_dir_all(&real).expect("mkdir");
        std::fs::write(wt.join(".git"), format!("gitdir: {}\n", real.display())).expect("file");
        std::fs::write(real.join("HEAD"), "ref: refs/heads/main\n").expect("head");
        assert_eq!(git_branch(&wt).as_deref(), Some("main"));
        let plain = dir.path().join("plain");
        std::fs::create_dir_all(&plain).expect("mkdir");
        assert_eq!(git_branch(&plain), None);
        assert_eq!(project_root(&plain), plain);
    }

    #[test]
    fn apps_by_binary() {
        assert_eq!(FerrumApp::from_binary("ferrumgit"), Some(FerrumApp::Git));
        assert_eq!(FerrumApp::from_binary("ferrumapi"), None);
    }

    #[test]
    fn the_ide_is_found_under_its_new_or_old_bundle_name() {
        let home = tempfile::tempdir().expect("home");
        let binary = |bundle: &str| {
            home.path()
                .join("Applications/Ferrum")
                .join(format!("{bundle}.app"))
                .join("Contents/MacOS/ferrumeditor")
        };
        let old = binary("Ferrum Editor");
        std::fs::create_dir_all(old.parent().expect("dir")).expect("mkdir");
        std::fs::write(&old, "").expect("old");
        assert_eq!(find_app(FerrumApp::Editor, Some(home.path())), Some(old));
        let new = binary("FerrumIDE");
        std::fs::create_dir_all(new.parent().expect("dir")).expect("mkdir");
        std::fs::write(&new, "").expect("new");
        assert_eq!(
            find_app(FerrumApp::Editor, Some(home.path())),
            Some(new),
            "the current name first"
        );
        let link = home.path().join(".ferrum/bin/ferrumeditor");
        std::fs::create_dir_all(link.parent().expect("dir")).expect("mkdir");
        std::fs::write(&link, "").expect("link");
        assert_eq!(
            find_app(FerrumApp::Editor, Some(home.path())),
            Some(link),
            "the launcher's link wins"
        );
    }

    #[test]
    fn editor_targets_carry_the_line() {
        let file = Path::new("/p/src/main.rs");
        assert_eq!(editor_target(file, Some(42)), "/p/src/main.rs:42");
        assert_eq!(editor_target(file, None), "/p/src/main.rs");
    }
}

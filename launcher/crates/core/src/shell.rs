//! Shell commands that open an app from a terminal, like `zed .`:
//! `fee .` opens Ferrum Editor on the current folder, `feg` FerrumGIT,
//! `fet` FerrumTerm in this folder, and so on. Each is a small script in
//! `~/.ferrum/bin` (next to the apps' own CLI links), turned on per app in
//! the launcher. Turning the first one on also puts `~/.ferrum/bin` on the
//! shell's PATH (one marked line in `~/.zprofile` and `~/.zshrc`,
//! `~/.bash_profile` and `~/.bashrc`, or fish's `conf.d`), so the commands
//! work in login and non-login terminals alike: Terminal, editor and IDE
//! terminals, tmux, FerrumTerm and Ferrum Editor's.

use std::path::{Path, PathBuf};

use crate::catalog::{AppId, AppInfo};
use crate::platform::Platform;
use crate::store::Paths;

/// First line after the shebang of every script this module writes; only
/// files with it are ever replaced or removed.
pub const MARKER: &str = "# Ferrum Launcher shell command";

/// Line added to shell startup files (with [`RC_COMMENT`] above it).
const RC_COMMENT: &str = "# Added by Ferrum Launcher: shell commands for Ferrum apps (fee, feg, …)";

/// The suggested command for an app: "Fe" plus its logo letter, lowercase.
/// The letters aren't unique (Ferrum Editor and FerrumEnv are both FeE), so
/// FerrumEnv keeps `fev` and Ferrum Editor `fee`.
#[must_use]
pub fn default_command(app: AppId) -> Option<&'static str> {
    Some(match app.as_str() {
        "editor" => "fee",
        "api" => "fea",
        "git" => "feg",
        "data" => "fed",
        "term" => "fet",
        "kit" => "fek",
        "dock" => "fec",
        "ports" => "fep",
        "env" => "fev",
        "agentic" => "fex",
        "launcher" => "fel",
        _ => return None,
    })
}

/// Checks a command name: 1–32 of `a-z 0-9 - _`, starting with a letter.
pub fn validate(name: &str) -> Result<(), String> {
    let valid = !name.is_empty()
        && name.len() <= 32
        && name.starts_with(|c: char| c.is_ascii_lowercase())
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-' || c == '_');
    if valid {
        Ok(())
    } else {
        Err(format!(
            "`{name}` can't be a command: use lowercase letters, digits, - or _, starting with a letter"
        ))
    }
}

/// Another program with this name on `path_env` (outside `bin_dir`), which
/// the command would hide or be hidden by.
#[must_use]
pub fn conflict(name: &str, bin_dir: &Path, path_env: &str) -> Option<PathBuf> {
    std::env::split_paths(path_env)
        .filter(|dir| dir != bin_dir)
        .map(|dir| dir.join(name))
        .find(|candidate| candidate.is_file())
}

fn quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}

/// The script for `app`'s command. It runs the app's CLI link in
/// `bin_dir` (which always points at the active version) in the
/// background, passing the arguments through; FerrumTerm's first opens a
/// session in the folder given (or the current one).
#[must_use]
pub fn script(info: &AppInfo, command: &str, entry: &Path) -> String {
    let target = quote(entry);
    let missing = format!(
        "[ -x {target} ] || {{ echo \"{command}: {name} is not installed. Install it in Ferrum Launcher.\" >&2; exit 1; }}\n",
        name = info.name
    );
    let launch = if info.id == AppId::TERM {
        format!(
            "dir=$(cd \"${{1:-.}}\" 2>/dev/null && pwd) || {{ echo \"{command}: no such folder: $1\" >&2; exit 1; }}\n\
             real=$(readlink {target} || echo {target})\n\
             fterm=\"$(dirname \"$real\")/fterm\"\n\
             [ -x \"$fterm\" ] && \"$fterm\" new \"$(basename \"$dir\")\" -c \"$dir\" >/dev/null 2>&1\n\
             cd \"$dir\" && nohup {target} >/dev/null 2>&1 &\n"
        )
    } else {
        format!("nohup {target} \"$@\" >/dev/null 2>&1 &\n")
    };
    format!(
        "#!/bin/sh\n{MARKER} for {name} (turn it off in Ferrum Launcher).\n{missing}{launch}",
        name = info.name
    )
}

fn is_ours(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .is_ok_and(|text| text.lines().nth(1).is_some_and(|l| l.starts_with(MARKER)))
}

/// Writes `command` for `app`. Refuses to replace a file it didn't write.
pub fn install(paths: &Paths, info: &AppInfo, command: &str) -> Result<PathBuf, String> {
    validate(command)?;
    if paths.platform == Platform::Windows {
        return Err("Shell commands are not available on Windows yet".into());
    }
    std::fs::create_dir_all(&paths.bin_dir).map_err(|e| e.to_string())?;
    let file = paths.bin_dir.join(command);
    if file.exists() && !is_ours(&file) {
        return Err(format!(
            "{} already exists and wasn't made by Ferrum Launcher",
            file.display()
        ));
    }
    let entry = paths.bin_dir.join(paths.platform.cli_entry(&info.binary));
    std::fs::write(&file, script(info, command, &entry)).map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| e.to_string())?;
    }
    Ok(file)
}

/// Removes `command` if this module wrote it.
pub fn remove(paths: &Paths, command: &str) -> Result<(), String> {
    let file = paths.bin_dir.join(command);
    if file.exists() && is_ours(&file) {
        std::fs::remove_file(&file).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// Shells whose startup files get the PATH line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shell {
    Zsh,
    Bash,
    Fish,
}

impl Shell {
    /// From `$SHELL` (zsh, macOS's default, when unknown).
    #[must_use]
    pub fn from_env(shell: Option<&str>) -> Self {
        match shell.and_then(|s| s.rsplit('/').next()) {
            Some("bash") => Shell::Bash,
            Some("fish") => Shell::Fish,
            _ => Shell::Zsh,
        }
    }

    /// The files that get the PATH line: the login file (read by
    /// Terminal's and FerrumTerm's login sessions and `$SHELL -l -c`) and
    /// the interactive one (read by editor/IDE terminals and tmux, which
    /// start non-login shells).
    #[must_use]
    pub fn startup_files(self, home: &Path) -> Vec<PathBuf> {
        match self {
            Shell::Zsh => vec![home.join(".zprofile"), home.join(".zshrc")],
            Shell::Bash => vec![home.join(".bash_profile"), home.join(".bashrc")],
            Shell::Fish => vec![home.join(".config/fish/conf.d/ferrum.fish")],
        }
    }

    fn path_line(self, bin_dir: &Path, home: &Path) -> String {
        let dir = match bin_dir.strip_prefix(home) {
            Ok(rest) => format!("$HOME/{}", rest.display()),
            Err(_) => bin_dir.display().to_string(),
        };
        match self {
            Shell::Fish => format!("fish_add_path --global \"{dir}\""),
            _ => format!("export PATH=\"{dir}:$PATH\""),
        }
    }
}

/// Adds `bin_dir` to the shell's PATH in each of its startup files that
/// does not mention it yet. Returns the files it changed.
pub fn ensure_on_path(bin_dir: &Path, home: &Path, shell: Shell) -> Result<Vec<PathBuf>, String> {
    let line = shell.path_line(bin_dir, home);
    let shown = bin_dir
        .strip_prefix(home)
        .map(|rest| rest.display().to_string())
        .unwrap_or_default();
    let mut changed = Vec::new();
    for file in shell.startup_files(home) {
        let existing = std::fs::read_to_string(&file).unwrap_or_default();
        if existing.contains(RC_COMMENT)
            || existing.contains(&bin_dir.display().to_string())
            || (!shown.is_empty() && existing.contains(&format!("$HOME/{shown}")))
        {
            continue;
        }
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut text = existing;
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        if !text.is_empty() {
            text.push('\n');
        }
        text.push_str(&format!("{RC_COMMENT}\n{line}\n"));
        std::fs::write(&file, text).map_err(|e| e.to_string())?;
        changed.push(file);
    }
    Ok(changed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Catalog;

    fn paths(dir: &Path) -> Paths {
        Paths {
            platform: Platform::MacOs,
            data_dir: dir.join("data"),
            bin_dir: dir.join("bin"),
            applications_dir: None,
            xdg_data_dir: None,
            start_menu_dir: None,
        }
    }

    fn info(app: AppId) -> AppInfo {
        Catalog::embedded().info(app).expect("built-in app")
    }

    #[test]
    fn defaults_follow_the_brand_letters() {
        assert_eq!(default_command(AppId::EDITOR), Some("fee"));
        assert_eq!(default_command(AppId::GIT), Some("feg"));
        assert_eq!(default_command(AppId::TERM), Some("fet"));
        assert_eq!(default_command(AppId::API), Some("fea"));
        assert_eq!(
            default_command(AppId::DOCK),
            Some("fec"),
            "FerrumContainers"
        );
        assert_eq!(
            default_command(AppId::ENV),
            Some("fev"),
            "fee is the Editor's"
        );
        assert_eq!(default_command(AppId::AGENTIC), Some("fex"), "FeX");
        let mut seen = std::collections::HashSet::new();
        for app in AppId::BUILTIN {
            let command = default_command(app).expect("default");
            validate(command).expect("valid");
            assert!(seen.insert(command), "{command} suggested twice");
        }
        assert!(validate("Fee").is_err());
        assert!(validate("1x").is_err());
        assert!(validate("a b").is_err());
        assert!(validate("").is_err());
    }

    #[test]
    fn scripts_run_the_active_cli_link() {
        let editor = info(AppId::EDITOR);
        let entry = Path::new("/Users/me/.ferrum/bin/ferrumeditor");
        let text = script(&editor, "fee", entry);
        assert!(text.starts_with("#!/bin/sh\n# Ferrum Launcher shell command for Ferrum Editor"));
        assert!(text.contains("nohup '/Users/me/.ferrum/bin/ferrumeditor' \"$@\""));
        let term = script(&info(AppId::TERM), "fet", Path::new("/b/ferrumterm"));
        assert!(term.contains("\"$fterm\" new \"$(basename \"$dir\")\" -c \"$dir\""));
    }

    #[cfg(unix)]
    #[test]
    fn install_runs_and_remove_only_touches_our_files() {
        let dir = tempfile::tempdir().expect("tempdir");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.bin_dir).expect("mkdir");
        // A fake app CLI that records its arguments and folder.
        let log = dir.path().join("ran.txt");
        let app = paths.bin_dir.join("ferrumeditor");
        std::fs::write(
            &app,
            format!("#!/bin/sh\necho \"$PWD $*\" > '{}'\n", log.display()),
        )
        .expect("write");
        {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(&app, std::fs::Permissions::from_mode(0o755)).expect("chmod");
        }
        let file = install(&paths, &info(AppId::EDITOR), "fee").expect("install");
        let work = dir.path().join("project");
        std::fs::create_dir_all(&work).expect("mkdir");
        let status = std::process::Command::new(&file)
            .arg(".")
            .current_dir(&work)
            .status()
            .expect("run");
        assert!(status.success());
        for _ in 0..100 {
            if log.exists() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(20));
        }
        let ran = std::fs::read_to_string(&log).expect("the app ran");
        assert!(ran.trim_end().ends_with(" ."), "{ran}");
        assert!(ran.contains("project"), "{ran}");
        // Someone else's file of the same name is left alone.
        std::fs::write(paths.bin_dir.join("feg"), "#!/bin/sh\n").expect("write");
        assert!(install(&paths, &info(AppId::GIT), "feg").is_err());
        remove(&paths, "feg").expect("remove");
        assert!(paths.bin_dir.join("feg").exists());
        remove(&paths, "fee").expect("remove");
        assert!(!file.exists());
    }

    #[test]
    fn path_line_is_added_once_per_shell() {
        let home = tempfile::tempdir().expect("tempdir");
        let bin = home.path().join(".ferrum/bin");
        // A .zshrc that already has the line (added by hand) is left alone.
        std::fs::write(
            home.path().join(".zshrc"),
            "export PATH=\"$HOME/.ferrum/bin:$PATH\"\n",
        )
        .expect("write");
        let changed = ensure_on_path(&bin, home.path(), Shell::Zsh).expect("zsh");
        assert_eq!(changed, vec![home.path().join(".zprofile")]);
        let text = std::fs::read_to_string(home.path().join(".zprofile")).expect("read");
        assert!(text.contains("export PATH=\"$HOME/.ferrum/bin:$PATH\""));
        std::fs::remove_file(home.path().join(".zshrc")).expect("remove");
        assert_eq!(
            ensure_on_path(&bin, home.path(), Shell::Zsh).expect("again"),
            vec![home.path().join(".zshrc")]
        );
        assert!(
            ensure_on_path(&bin, home.path(), Shell::Zsh)
                .expect("third")
                .is_empty()
        );
        std::fs::write(home.path().join(".bash_profile"), "alias ll='ls -l'").expect("write");
        ensure_on_path(&bin, home.path(), Shell::Bash).expect("bash");
        let bash = std::fs::read_to_string(home.path().join(".bash_profile")).expect("read");
        assert!(bash.starts_with("alias ll='ls -l'\n\n# Added by Ferrum Launcher"));
        ensure_on_path(&bin, home.path(), Shell::Fish).expect("fish");
        let fish = std::fs::read_to_string(home.path().join(".config/fish/conf.d/ferrum.fish"))
            .expect("read");
        assert!(fish.contains("fish_add_path --global \"$HOME/.ferrum/bin\""));
        assert_eq!(Shell::from_env(Some("/opt/homebrew/bin/fish")), Shell::Fish);
        assert_eq!(Shell::from_env(None), Shell::Zsh);
    }

    #[test]
    fn conflicts_are_found_outside_our_bin() {
        let dir = tempfile::tempdir().expect("tempdir");
        let other = dir.path().join("other");
        std::fs::create_dir_all(&other).expect("mkdir");
        std::fs::write(other.join("fed"), "").expect("write");
        let ours = dir.path().join("bin");
        let path = format!("{}:{}", ours.display(), other.display());
        assert_eq!(conflict("fed", &ours, &path), Some(other.join("fed")));
        assert_eq!(conflict("fee", &ours, &path), None);
    }
}

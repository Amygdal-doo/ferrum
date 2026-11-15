//! Shell commands that open an app from a terminal, like `zed .`:
//! `fei .` opens FerrumIDE on the current folder, `feg` FerrumGIT,
//! `fet` FerrumTerm in this folder, and so on. Each is a small script in
//! `~/.ferrum/bin` (next to the apps' own CLI links), turned on per app in
//! the launcher. Turning the first one on also puts `~/.ferrum/bin` on the
//! shell's PATH (one marked line in `~/.zprofile` and `~/.zshrc`,
//! `~/.bash_profile` and `~/.bashrc`, or fish's `conf.d`), so the commands
//! work in login and non-login terminals alike: Terminal, editor and IDE
//! terminals, tmux, FerrumTerm and FerrumIDE's.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::catalog::{AppId, AppInfo};
use crate::platform::Platform;
use crate::store::{AppPrefs, Paths};

/// First line after the shebang of every script this module writes; only
/// files with it are ever replaced or removed.
pub const MARKER: &str = "# Ferrum Launcher shell command";

/// Comment above the PATH line added to shell startup files. Its
/// "(fee, feg, …)" predates FerrumIDE's `fei` and is kept, because
/// [`ensure_on_path`] recognises the lines earlier launchers wrote by it.
const RC_COMMENT: &str = "# Added by Ferrum Launcher: shell commands for Ferrum apps (fee, feg, …)";

/// The suggested command for an app: "Fe" plus its logo letter, lowercase:
/// `fei` FerrumIDE (FeI), `fen` FerrumEngineer (FeN), `fev` FerrumVariables
/// (FeV). Before the 2026-11 renames they were `fee`, `fev` and `fen`; see
/// [`RENAMED_DEFAULTS`].
#[must_use]
pub fn default_command(app: AppId) -> Option<&'static str> {
    Some(match app.as_str() {
        "editor" => "fei",
        "api" => "fea",
        "git" => "feg",
        "data" => "fed",
        "term" => "fet",
        "kit" => "fek",
        "dock" => "fec",
        "ports" => "fep",
        "env" => "fev",
        "vibe" => "fen",
        "system" => "fes",
        "launcher" => "fel",
        _ => return None,
    })
}

/// Suggested commands that changed with the 2026-11 renames: (app, old,
/// new). Ferrum Editor's `fee` became FerrumIDE's `fei`; FerrumVibe's `fev`
/// became FerrumEngineer's `fen`, and FerrumEnv's `fen` FerrumVariables'
/// `fev` (the two swap).
pub const RENAMED_DEFAULTS: [(AppId, &str, &str); 3] = [
    (AppId::EDITOR, "fee", "fei"),
    (AppId::VIBE, "fev", "fen"),
    (AppId::ENV, "fen", "fev"),
];

/// Moves each app in [`RENAMED_DEFAULTS`] that still uses its old suggested
/// command to the new one. Names the user chose are left alone, and a move
/// that would give two apps the same command (the new name is someone
/// else's) is not made. Only changes the settings; [`sync`] then rewrites
/// the scripts. Returns the moves made.
pub fn rename_defaults(prefs: &mut BTreeMap<AppId, AppPrefs>) -> Vec<(AppId, String, String)> {
    let mut moved: Vec<(AppId, String, String)> = RENAMED_DEFAULTS
        .iter()
        .filter(|(app, old, _)| {
            prefs
                .get(app)
                .is_some_and(|p| p.command.as_deref() == Some(*old))
        })
        .map(|(app, old, new)| (*app, (*old).to_string(), (*new).to_string()))
        .collect();
    // Each command after the moves; a move whose new name is still held by
    // another app is dropped, which can free or take a name in turn.
    loop {
        let command_of = |app: AppId| {
            moved
                .iter()
                .find(|(a, _, _)| *a == app)
                .map(|(_, _, new)| new.clone())
                .or_else(|| prefs.get(&app).and_then(|p| p.command.clone()))
        };
        let clash = moved.iter().position(|(app, _, new)| {
            prefs
                .keys()
                .any(|other| other != app && command_of(*other).as_deref() == Some(new.as_str()))
        });
        match clash {
            Some(index) => {
                moved.remove(index);
            }
            None => break,
        }
    }
    for (app, _, new) in &moved {
        if let Some(p) = prefs.get_mut(app) {
            p.command = Some(new.clone());
        }
    }
    moved
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
    } else if info.id.as_str() == "dock" {
        // `fec` is FerrumContainers' command line (VM start/stop, compose,
        // docker); it opens the app itself for `fec` and `fec <folder>`.
        format!(
            "real=$(readlink {target} || echo {target})\n\
             cli=\"$(dirname \"$real\")/ferrumdock-cli\"\n\
             [ -x \"$cli\" ] && exec \"$cli\" \"$@\"\n\
             nohup {target} \"$@\" >/dev/null 2>&1 &\n"
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

/// Rewrites `command` with the current script if this module wrote it
/// (launcher updates change what a command does, e.g. `fec`).
pub fn refresh(paths: &Paths, info: &AppInfo, command: &str) -> Result<(), String> {
    let file = paths.bin_dir.join(command);
    if !file.exists() || !is_ours(&file) {
        return Ok(());
    }
    let entry = paths.bin_dir.join(paths.platform.cli_entry(&info.binary));
    let script = script(info, command, &entry);
    if std::fs::read_to_string(&file).is_ok_and(|current| current == script) {
        return Ok(());
    }
    std::fs::write(&file, script).map_err(|e| e.to_string())
}

/// Brings the scripts in `bin_dir` in line with `prefs`: every command
/// that is on gets its app's current script (written or rewritten), and a
/// script this module wrote whose name no app has any more is removed, so
/// a name that moved to another app (`fen`, `fev`) never opens the old one.
/// A command whose file was made by someone else is turned off. Run at
/// every start (it replaces launcher updates' scripts too, e.g. `fec`).
pub fn sync(
    paths: &Paths,
    prefs: &mut BTreeMap<AppId, AppPrefs>,
    info: impl Fn(AppId) -> AppInfo,
) -> Synced {
    let mut synced = Synced::default();
    if paths.platform == Platform::Windows {
        return synced;
    }
    let wanted: BTreeMap<String, AppId> = prefs
        .iter()
        .filter_map(|(app, p)| p.command.clone().map(|c| (c, *app)))
        .collect();
    if let Ok(entries) = std::fs::read_dir(&paths.bin_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            // Scripts are small regular files; CLI links are symlinks.
            let script = entry
                .path()
                .symlink_metadata()
                .is_ok_and(|m| m.is_file() && m.len() < 4096);
            if script && !wanted.contains_key(&name) && is_ours(&entry.path()) {
                match std::fs::remove_file(entry.path()) {
                    Ok(()) => synced.files_changed = true,
                    Err(error) => {
                        tracing::warn!(%error, name, "could not remove a stale shell command");
                    }
                }
            }
        }
    }
    for (command, app) in wanted {
        let info = info(app);
        let file = paths.bin_dir.join(&command);
        let entry = paths.bin_dir.join(paths.platform.cli_entry(&info.binary));
        let script = script(&info, &command, &entry);
        if std::fs::read_to_string(&file).is_ok_and(|current| current == script) {
            continue;
        }
        match install(paths, &info, &command) {
            Ok(_) => synced.files_changed = true,
            Err(error) => {
                tracing::warn!(%error, command, "turning a shell command off");
                if let Some(p) = prefs.get_mut(&app) {
                    p.command = None;
                }
                synced.turned_off.push(app);
            }
        }
    }
    synced
}

/// What [`sync`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Synced {
    /// A script was written, rewritten or removed.
    pub files_changed: bool,
    /// Apps whose command was turned off: its name is someone else's file.
    pub turned_off: Vec<AppId>,
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
        assert_eq!(default_command(AppId::EDITOR), Some("fei"), "FeI");
        assert_eq!(default_command(AppId::GIT), Some("feg"));
        assert_eq!(default_command(AppId::TERM), Some("fet"));
        assert_eq!(default_command(AppId::API), Some("fea"));
        assert_eq!(
            default_command(AppId::DOCK),
            Some("fec"),
            "FerrumContainers"
        );
        assert_eq!(default_command(AppId::ENV), Some("fev"), "FeV");
        assert_eq!(default_command(AppId::VIBE), Some("fen"), "FeN");
        assert_eq!(default_command(AppId::SYSTEM), Some("fes"), "FeS");
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
    fn fec_runs_the_containers_command_line() {
        let text = script(&info(AppId::DOCK), "fec", Path::new("/b/ferrumdock"));
        assert!(text.contains("cli=\"$(dirname \"$real\")/ferrumdock-cli\""));
        assert!(
            text.contains("exec \"$cli\" \"$@\""),
            "in the foreground, output kept"
        );
        assert!(
            text.contains("nohup '/b/ferrumdock'"),
            "older apps without the CLI"
        );
    }

    #[test]
    fn refresh_rewrites_only_its_own_scripts() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.bin_dir).expect("bin");
        let dock = info(AppId::DOCK);
        let fec = paths.bin_dir.join("fec");
        std::fs::write(
            &fec,
            format!("#!/bin/sh\n{MARKER} for FerrumContainers\nold\n"),
        )
        .expect("write");
        refresh(&paths, &dock, "fec").expect("refresh");
        assert!(
            std::fs::read_to_string(&fec)
                .expect("read")
                .contains("ferrumdock-cli")
        );
        let mine = paths.bin_dir.join("mine");
        std::fs::write(&mine, "#!/bin/sh\necho mine\n").expect("write");
        refresh(&paths, &dock, "mine").expect("refresh");
        assert_eq!(
            std::fs::read_to_string(&mine).expect("read"),
            "#!/bin/sh\necho mine\n"
        );
    }

    #[test]
    fn scripts_run_the_active_cli_link() {
        let editor = info(AppId::EDITOR);
        let entry = Path::new("/Users/me/.ferrum/bin/ferrumeditor");
        let text = script(&editor, "fei", entry);
        assert!(text.starts_with("#!/bin/sh\n# Ferrum Launcher shell command for FerrumIDE"));
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
        let file = install(&paths, &info(AppId::EDITOR), "fei").expect("install");
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
        remove(&paths, "fei").expect("remove");
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
        assert_eq!(conflict("fei", &ours, &path), None);
    }

    fn prefs(commands: &[(AppId, &str)]) -> BTreeMap<AppId, AppPrefs> {
        commands
            .iter()
            .map(|(app, command)| {
                let prefs = AppPrefs {
                    command: Some((*command).into()),
                    ..AppPrefs::default()
                };
                (*app, prefs)
            })
            .collect()
    }

    fn commands(prefs: &BTreeMap<AppId, AppPrefs>) -> Vec<(AppId, Option<&str>)> {
        prefs
            .iter()
            .map(|(app, p)| (*app, p.command.as_deref()))
            .collect()
    }

    #[test]
    fn renamed_defaults_move_and_fen_fev_swap() {
        let mut prefs = prefs(&[
            (AppId::EDITOR, "fee"),
            (AppId::VIBE, "fev"),
            (AppId::ENV, "fen"),
            (AppId::GIT, "feg"),
        ]);
        let moved = rename_defaults(&mut prefs);
        assert_eq!(moved.len(), 3, "{moved:?}");
        assert_eq!(
            commands(&prefs),
            [
                (AppId::EDITOR, Some("fei")),
                (AppId::ENV, Some("fev")),
                (AppId::GIT, Some("feg")),
                (AppId::VIBE, Some("fen")),
            ]
            .into_iter()
            .collect::<BTreeMap<_, _>>()
            .into_iter()
            .collect::<Vec<_>>()
        );
        assert!(
            rename_defaults(&mut prefs).is_empty(),
            "nothing left to move"
        );
        // Only one of the pair on: it still moves.
        let mut prefs = prefs_one(AppId::ENV, "fen");
        rename_defaults(&mut prefs);
        assert_eq!(prefs[&AppId::ENV].command.as_deref(), Some("fev"));
        let mut prefs = prefs_one(AppId::VIBE, "fev");
        rename_defaults(&mut prefs);
        assert_eq!(prefs[&AppId::VIBE].command.as_deref(), Some("fen"));
    }

    fn prefs_one(app: AppId, command: &str) -> BTreeMap<AppId, AppPrefs> {
        prefs(&[(app, command)])
    }

    #[test]
    fn renamed_defaults_leave_chosen_names_and_never_clash() {
        // Names the user chose stay.
        let mut chosen = prefs(&[(AppId::EDITOR, "ed"), (AppId::ENV, "secrets")]);
        assert!(rename_defaults(&mut chosen).is_empty());
        // FerrumKit already has `fei`: the IDE keeps `fee`.
        let mut taken = prefs(&[(AppId::EDITOR, "fee"), (AppId::KIT, "fei")]);
        assert!(rename_defaults(&mut taken).is_empty());
        assert_eq!(taken[&AppId::EDITOR].command.as_deref(), Some("fee"));
        // FerrumKit has `fen` (so FerrumVariables has another name or none):
        // FerrumEngineer keeps `fev`.
        let mut blocked = prefs(&[(AppId::VIBE, "fev"), (AppId::KIT, "fen")]);
        assert!(rename_defaults(&mut blocked).is_empty());
        assert_eq!(blocked[&AppId::VIBE].command.as_deref(), Some("fev"));
        let mut both = prefs(&[
            (AppId::VIBE, "fev"),
            (AppId::ENV, "fen"),
            (AppId::EDITOR, "fee"),
            (AppId::KIT, "fei"),
        ]);
        let moved = rename_defaults(&mut both);
        assert_eq!(moved.len(), 2, "{moved:?}");
        assert_eq!(both[&AppId::EDITOR].command.as_deref(), Some("fee"));
        let mut names: Vec<_> = both.values().filter_map(|p| p.command.clone()).collect();
        names.sort();
        names.dedup();
        assert_eq!(names.len(), both.len(), "every command is unique");
    }

    fn write_script(paths: &Paths, app: AppId, command: &str) {
        let info = info(app);
        let entry = paths.bin_dir.join(&info.binary);
        std::fs::write(paths.bin_dir.join(command), script(&info, command, &entry)).expect("write");
    }

    fn opens(paths: &Paths, command: &str) -> Option<String> {
        let text = std::fs::read_to_string(paths.bin_dir.join(command)).ok()?;
        [
            "ferrumeditor",
            "ferrumvibe",
            "ferrumenv",
            "ferrumagentic",
            "ferrumgit",
        ]
        .into_iter()
        .find(|binary| text.contains(&format!("/{binary}'")))
        .map(str::to_string)
    }

    #[cfg(unix)]
    #[test]
    fn sync_moves_the_scripts_with_the_renamed_commands() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.bin_dir).expect("bin");
        // What a launcher before the renames left: fee → editor, fen → env,
        // fev → vibe, a stale fex, the user's own script and a CLI link.
        write_script(&paths, AppId::EDITOR, "fee");
        write_script(&paths, AppId::ENV, "fen");
        write_script(&paths, AppId::VIBE, "fev");
        write_script(&paths, AppId::GIT, "feg");
        std::fs::write(
            paths.bin_dir.join("fex"),
            format!("#!/bin/sh\n{MARKER} for FerrumAgentic\nferrumagentic\n"),
        )
        .expect("fex");
        std::fs::write(paths.bin_dir.join("mine"), "#!/bin/sh\necho mine\n").expect("mine");
        std::os::unix::fs::symlink(dir.path().join("app"), paths.bin_dir.join("ferrumeditor"))
            .expect("link");
        let mut prefs = prefs(&[
            (AppId::EDITOR, "fee"),
            (AppId::VIBE, "fev"),
            (AppId::ENV, "fen"),
            (AppId::GIT, "feg"),
        ]);
        rename_defaults(&mut prefs);
        let synced = sync(&paths, &mut prefs, info);
        assert!(synced.files_changed);
        assert!(synced.turned_off.is_empty());
        assert_eq!(opens(&paths, "fei").as_deref(), Some("ferrumeditor"));
        assert_eq!(opens(&paths, "fen").as_deref(), Some("ferrumvibe"));
        assert_eq!(opens(&paths, "fev").as_deref(), Some("ferrumenv"));
        assert_eq!(opens(&paths, "feg").as_deref(), Some("ferrumgit"));
        assert!(!paths.bin_dir.join("fee").exists(), "fee is gone");
        assert!(!paths.bin_dir.join("fex").exists(), "stale script removed");
        assert!(paths.bin_dir.join("mine").exists(), "not ours");
        assert!(
            paths.bin_dir.join("ferrumeditor").is_symlink(),
            "CLI link kept"
        );
        let fen = std::fs::read_to_string(paths.bin_dir.join("fen")).expect("fen");
        assert!(fen.contains("for FerrumEngineer"), "{fen}");
        // Nothing to do the second time.
        assert_eq!(sync(&paths, &mut prefs, info), Synced::default());
    }

    #[cfg(unix)]
    #[test]
    fn sync_turns_off_a_command_whose_name_is_someone_elses_file() {
        let dir = tempfile::tempdir().expect("tmp");
        let paths = paths(dir.path());
        std::fs::create_dir_all(&paths.bin_dir).expect("bin");
        std::fs::write(paths.bin_dir.join("fei"), "#!/bin/sh\necho mine\n").expect("fei");
        write_script(&paths, AppId::EDITOR, "fee");
        let mut prefs = prefs(&[(AppId::EDITOR, "fee")]);
        rename_defaults(&mut prefs);
        let synced = sync(&paths, &mut prefs, info);
        assert_eq!(synced.turned_off, [AppId::EDITOR]);
        assert_eq!(prefs[&AppId::EDITOR].command, None);
        assert_eq!(
            std::fs::read_to_string(paths.bin_dir.join("fei")).expect("fei"),
            "#!/bin/sh\necho mine\n"
        );
        assert!(
            !paths.bin_dir.join("fee").exists(),
            "no longer opens the IDE"
        );
    }
}

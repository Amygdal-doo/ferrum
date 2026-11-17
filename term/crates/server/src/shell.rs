//! Shell integration: scripts that make the shell mark each prompt and
//! command (OSC 133), which the emulator turns into command blocks.
//!
//! The server writes them to `<state dir>/shell-integration/` and loads them
//! into new shells by itself: zsh through a `ZDOTDIR` whose `.zshenv` puts
//! the user's `ZDOTDIR` back and then sources theirs (so `.zshrc` and the
//! rest load as usual), fish (before 4, which marks prompts itself) as a
//! `vendor_conf.d` script found through `XDG_DATA_DIRS`. bash has no such
//! hook: `eval "$(fterm shell-integration bash)"` in `~/.bashrc` adds it.
//! `FTERM_SHELL_INTEGRATION=0` (in the block's or the server's environment)
//! turns the automatic loading off.

use std::path::{Path, PathBuf};

const ZSH: &str = include_str!("shell/ferrumterm.zsh");
const ZSHENV: &str = include_str!("shell/zshenv");
const BASH: &str = include_str!("shell/ferrumterm.bash");
const FISH: &str = include_str!("shell/ferrumterm.fish");

/// The integration script for `shell` (`zsh`, `bash`, `fish`), to source.
#[must_use]
pub fn script(shell: &str) -> Option<&'static str> {
    match shell {
        "zsh" => Some(ZSH),
        "bash" => Some(BASH),
        "fish" => Some(FISH),
        _ => None,
    }
}

/// Writes the scripts under `dir` (only files whose content changed).
pub fn install(dir: &Path) -> std::io::Result<()> {
    for (file, text) in [
        ("zsh/.zshenv", ZSHENV),
        ("zsh/ferrumterm.zsh", ZSH),
        ("bash/ferrumterm.bash", BASH),
        ("fish/fish/vendor_conf.d/ferrumterm.fish", FISH),
    ] {
        let path = dir.join(file);
        if std::fs::read_to_string(&path).is_ok_and(|old| old == text) {
            continue;
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&path, text)?;
    }
    Ok(())
}

/// The shell a block runs: its program's name (`/bin/zsh`, `-zsh` → `zsh`),
/// or the login shell (`$SHELL`) when it starts the default program.
fn shell_name(command: &[String], var: &dyn Fn(&str) -> Option<String>) -> Option<String> {
    let program = match command.first() {
        Some(p) => p.clone(),
        None => var("SHELL")?,
    };
    let name = Path::new(&program)
        .file_name()?
        .to_string_lossy()
        .to_string();
    Some(name.trim_start_matches('-').to_owned())
}

/// Environment that loads the integration into the block's shell, given
/// the block's environment `var` (its spawn environment, else the
/// server's) and the scripts' folder.
#[must_use]
pub fn env(
    command: &[String],
    dir: &Path,
    var: &dyn Fn(&str) -> Option<String>,
) -> Vec<(String, String)> {
    if var("FTERM_SHELL_INTEGRATION").is_some_and(|v| matches!(v.trim(), "0" | "false" | "off")) {
        return Vec::new();
    }
    let path = |p: PathBuf| p.display().to_string();
    match shell_name(command, var).as_deref() {
        Some("zsh") => {
            let mut env = vec![("ZDOTDIR".to_owned(), path(dir.join("zsh")))];
            if let Some(original) = var("ZDOTDIR") {
                env.push(("FTERM_ZSH_ZDOTDIR".to_owned(), original));
            }
            env
        }
        Some("fish") => {
            let original = var("XDG_DATA_DIRS");
            let rest = original
                .clone()
                .filter(|d| !d.is_empty())
                .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
            vec![
                (
                    "XDG_DATA_DIRS".to_owned(),
                    format!("{}:{rest}", path(dir.join("fish"))),
                ),
                (
                    "FTERM_FISH_XDG_DATA_DIRS".to_owned(),
                    original.unwrap_or_default(),
                ),
            ]
        }
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |k| pairs.iter().find(|(n, _)| n == k).map(|(_, v)| v.clone())
    }

    #[test]
    fn loads_into_zsh_and_fish_keeping_their_settings() {
        let dir = Path::new("/s");
        let login = vars(&[("SHELL", "/bin/zsh")]);
        assert_eq!(
            env(&[], dir, &login),
            [("ZDOTDIR".to_owned(), "/s/zsh".to_owned())]
        );
        let own = vars(&[("ZDOTDIR", "/home/me/.config/zsh")]);
        let zsh = env(&["-zsh".to_owned()], dir, &own);
        assert!(zsh.contains(&("FTERM_ZSH_ZDOTDIR".into(), "/home/me/.config/zsh".into())));
        let fish = env(&["/opt/homebrew/bin/fish".to_owned()], dir, &vars(&[]));
        assert_eq!(fish[0].1, "/s/fish:/usr/local/share:/usr/share");
        assert_eq!(fish[1], ("FTERM_FISH_XDG_DATA_DIRS".into(), String::new()));
        // Other programs, bash, and turned off: nothing.
        assert!(env(&["python3".to_owned()], dir, &login).is_empty());
        assert!(env(&[], dir, &vars(&[("SHELL", "/bin/bash")])).is_empty());
        let off = vars(&[("SHELL", "/bin/zsh"), ("FTERM_SHELL_INTEGRATION", "0")]);
        assert!(env(&[], dir, &off).is_empty());
    }

    #[test]
    fn installs_every_script_once() {
        let dir = tempfile::tempdir().expect("tempdir");
        install(dir.path()).expect("install");
        let zshenv = dir.path().join("zsh/.zshenv");
        assert_eq!(std::fs::read_to_string(&zshenv).expect("zshenv"), ZSHENV);
        assert!(
            dir.path()
                .join("fish/fish/vendor_conf.d/ferrumterm.fish")
                .is_file()
        );
        let before = std::fs::metadata(&zshenv).and_then(|m| m.modified()).ok();
        install(dir.path()).expect("again");
        let after = std::fs::metadata(&zshenv).and_then(|m| m.modified()).ok();
        assert_eq!(before, after, "unchanged files are left alone");
        assert!(script("zsh").is_some_and(|s| s.contains("133;C")));
        assert!(script("tcsh").is_none());
    }
}

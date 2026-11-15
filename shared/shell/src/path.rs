//! Putting `~/.ferrum/bin` on the shell's PATH: one marked line in
//! `~/.zprofile` and `~/.zshrc`, `~/.bash_profile` and `~/.bashrc`, or
//! fish's `conf.d`, so the commands work in login and non-login terminals
//! alike (Terminal, editor and IDE terminals, tmux, FerrumTerm).

use std::path::{Path, PathBuf};

/// Comment above the PATH line. Its "(fee, feg, …)" predates FerrumIDE's
/// `fei` and is kept: lines earlier launchers wrote are recognised by it.
const RC_COMMENT: &str = "# Added by Ferrum Launcher: shell commands for Ferrum apps (fee, feg, …)";

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

    /// The login file (read by Terminal's and FerrumTerm's login sessions)
    /// and the interactive one (editor/IDE terminals and tmux).
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

fn mentions(text: &str, bin_dir: &Path, home: &Path) -> bool {
    let shown = bin_dir
        .strip_prefix(home)
        .map(|rest| rest.display().to_string())
        .unwrap_or_default();
    text.contains(RC_COMMENT)
        || text.contains(&bin_dir.display().to_string())
        || (!shown.is_empty() && text.contains(&format!("$HOME/{shown}")))
}

/// Whether every startup file of `shell` already puts `bin_dir` on PATH.
#[must_use]
pub fn is_on_path(bin_dir: &Path, home: &Path, shell: Shell) -> bool {
    shell
        .startup_files(home)
        .iter()
        .all(|file| std::fs::read_to_string(file).is_ok_and(|text| mentions(&text, bin_dir, home)))
}

/// Adds `bin_dir` to the shell's PATH in each of its startup files that
/// does not mention it yet. Returns the files it changed.
pub fn ensure_on_path(bin_dir: &Path, home: &Path, shell: Shell) -> Result<Vec<PathBuf>, String> {
    let line = shell.path_line(bin_dir, home);
    let mut changed = Vec::new();
    for file in shell.startup_files(home) {
        let existing = std::fs::read_to_string(&file).unwrap_or_default();
        if mentions(&existing, bin_dir, home) {
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

//! The changes Settings › System (every app) and the launcher's app pages
//! make: each reads `preferences.json`, changes it, saves it and brings the
//! scripts in `~/.ferrum/bin` in line at once.

use std::path::{Path, PathBuf};

use crate::apps;
use crate::path::{Shell, ensure_on_path, is_on_path};
use crate::prefs::{self, SharedPrefs};
use crate::script;

/// Where the files are and the shell they are for.
#[derive(Debug, Clone)]
pub struct Env {
    /// `~/.ferrum` (`$FERRUM_HOME` in tests and portable installs).
    pub ferrum_dir: PathBuf,
    /// For the PATH line; `None` skips it.
    pub home: Option<PathBuf>,
    pub shell: Shell,
    /// `$PATH`, to warn about other programs of the same name.
    pub path_var: String,
}

impl Env {
    /// This process's `$HOME`, `$SHELL` and `$PATH`.
    #[must_use]
    pub fn from_process(ferrum_dir: PathBuf) -> Self {
        Self {
            ferrum_dir,
            home: std::env::var_os("HOME").map(PathBuf::from),
            shell: Shell::from_env(std::env::var("SHELL").ok().as_deref()),
            path_var: std::env::var("PATH").unwrap_or_default(),
        }
    }

    #[must_use]
    pub fn bin_dir(&self) -> PathBuf {
        self.ferrum_dir.join("bin")
    }

    #[must_use]
    pub fn prefs_path(&self) -> PathBuf {
        prefs::path(&self.ferrum_dir)
    }

    /// Whether new terminals find the commands.
    #[must_use]
    pub fn on_path(&self) -> bool {
        let bin = self.bin_dir();
        std::env::split_paths(&self.path_var).any(|dir| dir == bin)
            || self
                .home
                .as_deref()
                .is_some_and(|home| is_on_path(&bin, home, self.shell))
    }
}

/// The preferences, with the commands found in `bin_dir` taken over when
/// the file has none yet (a launcher before the shared file wrote them).
#[must_use]
pub fn load(prefs_path: &Path, bin_dir: &Path) -> SharedPrefs {
    let mut prefs = SharedPrefs::load(prefs_path);
    if prefs.shell_commands.is_none() {
        let found = script::discover(bin_dir)
            .into_iter()
            .map(|(app, name)| (app.to_owned(), name));
        prefs.migrate_from(found, 0);
        prefs.apply_renames();
    }
    prefs
}

/// What [`sync`] did.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Synced {
    pub files_changed: bool,
    /// Apps whose command was turned off: its name is someone else's file.
    pub turned_off: Vec<(String, String)>,
    /// Why `ferrum` could not be written.
    pub umbrella_failed: Option<String>,
}

/// Writes the scripts `prefs` asks for and removes the stale ones; a
/// command that can't be written is turned off in `prefs` (the caller
/// saves).
pub fn sync(bin_dir: &Path, prefs: &mut SharedPrefs) -> Synced {
    let done = script::sync(bin_dir, &prefs.wanted_scripts(bin_dir));
    let mut synced = Synced {
        files_changed: done.files_changed,
        ..Synced::default()
    };
    for (name, reason) in done.failed {
        if name == crate::umbrella::NAME {
            synced.umbrella_failed = Some(reason);
            continue;
        }
        let owner = apps::APPS
            .iter()
            .find(|app| prefs.enabled_command(app.id).as_deref() == Some(name.as_str()));
        if let Some(app) = owner {
            prefs.set_command(app.id, &name, false);
            synced.turned_off.push((app.id.to_owned(), reason));
        }
    }
    synced
}

fn save_and_sync(env: &Env, prefs: &mut SharedPrefs) -> Result<Synced, String> {
    let synced = sync(&env.bin_dir(), prefs);
    prefs
        .save(&env.prefs_path())
        .map_err(|e| format!("Could not save {}: {e}", env.prefs_path().display()))?;
    Ok(synced)
}

/// "(Added ~/.ferrum/bin to PATH in ~/.zprofile and ~/.zshrc.)", or "".
fn add_to_path(env: &Env) -> Result<String, String> {
    let Some(home) = env.home.as_deref() else {
        return Ok(String::new());
    };
    let files = ensure_on_path(&env.bin_dir(), home, env.shell)
        .map_err(|e| format!("PATH could not be updated: {e}. Add ~/.ferrum/bin to your PATH."))?;
    if files.is_empty() {
        return Ok(String::new());
    }
    let names: Vec<String> = files
        .iter()
        .map(|file| match file.strip_prefix(home) {
            Ok(rest) => format!("~/{}", rest.display()),
            Err(_) => file.display().to_string(),
        })
        .collect();
    Ok(format!(
        " (Added ~/.ferrum/bin to PATH in {}.)",
        names.join(" and ")
    ))
}

/// Puts `~/.ferrum/bin` on PATH (Settings › System's button).
pub fn add_bin_to_path(env: &Env) -> Result<String, String> {
    let added = add_to_path(env)?;
    Ok(format!(
        "Open a new terminal window to use the Ferrum commands.{added}"
    ))
}

/// Names `app_id`'s command `name` and turns it on or off; the script is
/// written or removed at once. Returns the message to show.
pub fn set_command(env: &Env, app_id: &str, name: &str, enabled: bool) -> Result<String, String> {
    let app = apps::by_id(app_id).ok_or_else(|| format!("`{app_id}` has no shell command"))?;
    let bin_dir = env.bin_dir();
    let mut prefs = load(&env.prefs_path(), &bin_dir);
    let current = prefs.command(app_id);
    let name = name.trim();
    let name = if name.is_empty() {
        current.name.as_str()
    } else {
        name
    };
    if name != current.name || enabled {
        crate::names::check(app_id, name, &prefs, &bin_dir)?;
    }
    let name = name.to_owned();
    prefs.set_command(app_id, &name, enabled);
    let synced = save_and_sync(env, &mut prefs)?;
    if let Some((_, reason)) = synced.turned_off.into_iter().find(|(a, _)| a == app_id) {
        return Err(reason);
    }
    if !enabled {
        return Ok(format!("The `{name}` command is off"));
    }
    let usage = app.usage(&name);
    let mut message = match add_to_path(env) {
        Ok(added) => format!("{usage}. Open a new terminal window to use it.{added}"),
        Err(error) => format!("{usage}, but {error}"),
    };
    if let Some(other) = script::conflict(&name, &bin_dir, &env.path_var) {
        message.push_str(&format!(" Note: {} also exists.", other.display()));
    }
    Ok(message)
}

/// Turns `ferrum <app>` on or off.
pub fn set_umbrella(env: &Env, enabled: bool) -> Result<String, String> {
    let mut prefs = load(&env.prefs_path(), &env.bin_dir());
    prefs.ferrum_command = Some(enabled);
    let synced = save_and_sync(env, &mut prefs)?;
    if let Some(reason) = synced.umbrella_failed {
        return Err(reason);
    }
    if !enabled {
        return Ok("The `ferrum` command is off".into());
    }
    let added = add_to_path(env)?;
    Ok(format!(
        "`ferrum ide .`, `ferrum git`, `ferrum list`… open the apps. Open a new terminal window to use it.{added}"
    ))
}

/// Records whether `binary` runs without a Dock icon.
pub fn set_hidden(prefs_path: &Path, binary: &str, hidden: bool) -> std::io::Result<()> {
    let mut prefs = SharedPrefs::load(prefs_path);
    prefs.set_hidden(binary, hidden);
    prefs.save(prefs_path)
}

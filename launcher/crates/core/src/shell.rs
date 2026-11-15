//! Shell commands that open an app from a terminal, like `zed .`: `fei .`
//! opens FerrumIDE on the current folder, `feg` FerrumGIT, `fet` FerrumTerm
//! in this folder, … and `ferrum <app> [path]` any of them. The scripts,
//! names and `~/.ferrum/preferences.json` (where the choices live, shared
//! with every app's Settings › System) are [`ferrum_shell`]'s; this module
//! adapts them to the launcher's types.

use std::collections::BTreeMap;

pub use ferrum_shell::ops::{set_command, set_hidden, set_umbrella};
pub use ferrum_shell::path::{Shell, ensure_on_path};
pub use ferrum_shell::script::conflict;
pub use ferrum_shell::{Env, MARKER, SharedPrefs, validate};

use crate::catalog::AppId;
use crate::store::{AppPrefs, Paths};

/// The suggested command for an app: "fe" plus its logo letter (`fei`
/// FerrumIDE, `fen` FerrumEngineer, `fev` FerrumVariables).
#[must_use]
pub fn default_command(app: AppId) -> Option<&'static str> {
    ferrum_shell::apps::by_id(app.as_str()).map(|app| app.default_command)
}

/// The shared helpers' view of `paths`: `~/.ferrum` is `bin_dir`'s parent.
#[must_use]
pub fn env(paths: &Paths) -> Option<Env> {
    Some(Env::from_process(paths.bin_dir.parent()?.to_path_buf()))
}

/// Moves each app that still uses the suggested command from before the
/// 2026-11 renames to the new one (see
/// [`ferrum_shell::prefs::RENAMED_DEFAULTS`]), in the launcher's own
/// settings (`state.json` before the shared file). Returns the moves made.
pub fn rename_defaults(prefs: &mut BTreeMap<AppId, AppPrefs>) -> Vec<(AppId, String, String)> {
    let names = prefs
        .iter()
        .filter_map(|(app, p)| Some((app.as_str().to_owned(), p.command.clone()?)))
        .collect();
    let mut moved = Vec::new();
    for (app, old, new) in ferrum_shell::prefs::rename_defaults(&names) {
        let Some(app) = AppId::new(&app) else {
            continue;
        };
        if let Some(p) = prefs.get_mut(&app) {
            p.command = Some(new.clone());
        }
        moved.push((app, old, new));
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn every_builtin_app_has_a_suggested_command() {
        for app in AppId::BUILTIN {
            let command = default_command(app).expect("default");
            validate(command).expect("valid");
        }
        assert_eq!(default_command(AppId::EDITOR), Some("fei"));
        assert_eq!(default_command(AppId::DOCK), Some("fec"));
        assert_eq!(default_command(AppId::LEGACY_VIBE), None);
    }

    #[test]
    fn renamed_defaults_move_in_the_launchers_settings() {
        let mut prefs = prefs(&[
            (AppId::EDITOR, "fee"),
            (AppId::VIBE, "fev"),
            (AppId::ENV, "fen"),
            (AppId::GIT, "feg"),
        ]);
        prefs.insert(AppId::KIT, AppPrefs::default());
        assert_eq!(rename_defaults(&mut prefs).len(), 3);
        let command = |app: AppId| prefs[&app].command.as_deref();
        assert_eq!(command(AppId::EDITOR), Some("fei"));
        assert_eq!(command(AppId::VIBE), Some("fen"));
        assert_eq!(command(AppId::ENV), Some("fev"));
        assert_eq!(command(AppId::GIT), Some("feg"));
        assert_eq!(command(AppId::KIT), None);
        // FerrumKit already has `fei`: the IDE keeps `fee`.
        let mut taken = self::tests::prefs(&[(AppId::EDITOR, "fee"), (AppId::KIT, "fei")]);
        assert!(rename_defaults(&mut taken).is_empty());
        assert_eq!(taken[&AppId::EDITOR].command.as_deref(), Some("fee"));
    }

    #[test]
    fn env_is_the_folder_holding_bin() {
        let paths = Paths {
            platform: crate::platform::Platform::MacOs,
            data_dir: "/d".into(),
            bin_dir: "/h/.ferrum/bin".into(),
            applications_dir: None,
            xdg_data_dir: None,
            start_menu_dir: None,
        };
        let env = env(&paths).expect("env");
        assert_eq!(env.bin_dir(), paths.bin_dir);
        assert_eq!(
            env.prefs_path(),
            std::path::Path::new("/h/.ferrum/preferences.json")
        );
    }
}

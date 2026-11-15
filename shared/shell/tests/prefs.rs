//! preferences.json, migration and the changes Settings makes.

use std::collections::BTreeMap;
use std::path::PathBuf;

use ferrum_shell::ops::{self, Env};
use ferrum_shell::prefs::{COMMANDS_REVISION, rename_defaults};
use ferrum_shell::{SharedPrefs, Shell};

fn names(pairs: &[(&str, &str)]) -> BTreeMap<String, String> {
    pairs
        .iter()
        .map(|(a, n)| ((*a).to_owned(), (*n).to_owned()))
        .collect()
}

#[test]
fn renamed_defaults_move_and_fen_fev_swap() {
    let moved = rename_defaults(&names(&[
        ("editor", "fee"),
        ("vibe", "fev"),
        ("env", "fen"),
        ("git", "feg"),
    ]));
    assert_eq!(moved.len(), 3, "{moved:?}");
    // Only one of the pair: it still moves.
    assert_eq!(rename_defaults(&names(&[("env", "fen")])).len(), 1);
    // Names the user chose stay.
    assert!(rename_defaults(&names(&[("editor", "ed"), ("env", "secrets")])).is_empty());
    // FerrumKit already has `fei`: the IDE keeps `fee`.
    assert!(rename_defaults(&names(&[("editor", "fee"), ("kit", "fei")])).is_empty());
    // FerrumKit has `fen`: FerrumEngineer keeps `fev`, and so FerrumVariables
    // can't take `fev` either.
    let blocked = rename_defaults(&names(&[("vibe", "fev"), ("env", "fen"), ("kit", "fen")]));
    assert!(blocked.is_empty(), "{blocked:?}");
}

#[test]
fn file_keeps_unknown_fields_and_reads_defaults() {
    let dir = tempfile::tempdir().expect("tmp");
    let path = dir.path().join("preferences.json");
    std::fs::write(
        &path,
        r#"{"hide_from_dock":["ferrumgit"],"future":{"x":1}}"#,
    )
    .expect("write");
    let mut prefs = SharedPrefs::load(&path);
    assert!(prefs.hide_from_dock.contains("ferrumgit"));
    assert!(prefs.shell_commands.is_none(), "not migrated yet");
    assert!(prefs.umbrella_enabled(), "on by default");
    assert_eq!(prefs.command("editor").name, "fei");
    assert!(!prefs.command("editor").enabled);
    prefs.set_command("editor", "ide", true);
    prefs.set_hidden("ferrumgit", false);
    prefs.save(&path).expect("save");
    let text = std::fs::read_to_string(&path).expect("read");
    assert!(text.contains("\"future\""), "{text}");
    assert!(text.contains("\"shell_commands\""), "{text}");
    let again = SharedPrefs::load(&path);
    assert_eq!(again.enabled_command("editor").as_deref(), Some("ide"));
    assert!(again.hide_from_dock.is_empty());
    std::fs::write(&path, "{broken").expect("write");
    assert_eq!(SharedPrefs::load(&path), SharedPrefs::default());
}

#[test]
fn migration_takes_the_launchers_commands_once_and_keeps_its_revision() {
    // A launcher state at the current revision: taken as is.
    let mut prefs = SharedPrefs::default();
    assert!(prefs.migrate_from([("editor".to_owned(), "fei".to_owned())], COMMANDS_REVISION));
    assert!(prefs.apply_renames().is_empty());
    assert_eq!(prefs.enabled_command("editor").as_deref(), Some("fei"));
    // Never again: the file is the source of truth now.
    assert!(!prefs.migrate_from([("git".to_owned(), "g".to_owned())], 0));
    assert_eq!(prefs.enabled_command("git"), None);

    // Commands from before the renames (revision 0) move once.
    let mut old = SharedPrefs::default();
    old.migrate_from(
        [
            ("editor".to_owned(), "fee".to_owned()),
            ("vibe".to_owned(), "fev".to_owned()),
            ("env".to_owned(), "fen".to_owned()),
        ],
        0,
    );
    assert_eq!(old.apply_renames().len(), 3);
    assert_eq!(old.enabled_command("editor").as_deref(), Some("fei"));
    assert_eq!(old.enabled_command("vibe").as_deref(), Some("fen"));
    assert_eq!(old.enabled_command("env").as_deref(), Some("fev"));
    assert_eq!(old.commands_revision, COMMANDS_REVISION);
    // A later choice of `fen` for FerrumVariables isn't moved again.
    old.set_command("vibe", "fen", false);
    old.set_command("vibe", "eng", true);
    old.set_command("env", "fen", true);
    assert!(old.apply_renames().is_empty());
    assert_eq!(old.enabled_command("env").as_deref(), Some("fen"));
}

fn env(dir: &std::path::Path) -> Env {
    Env {
        ferrum_dir: dir.join(".ferrum"),
        home: Some(dir.to_path_buf()),
        shell: Shell::Zsh,
        path_var: String::new(),
    }
}

fn read(path: PathBuf) -> Option<String> {
    std::fs::read_to_string(path).ok()
}

#[cfg(unix)]
#[test]
fn settings_rename_toggle_and_umbrella_take_effect_at_once() {
    let dir = tempfile::tempdir().expect("tmp");
    let env = env(dir.path());
    let bin = env.bin_dir();
    // On: the script, the umbrella and the PATH line appear.
    let message = ops::set_command(&env, "editor", "", true).expect("on");
    assert!(message.contains("`fei .` opens FerrumIDE"), "{message}");
    assert!(message.contains("~/.zprofile"), "{message}");
    assert!(
        read(bin.join("fei"))
            .expect("fei")
            .contains("/ferrumeditor'")
    );
    let umbrella = read(bin.join("ferrum")).expect("ferrum");
    assert!(umbrella.contains("  editor|fei|ide)\n"), "{umbrella}");
    assert!(env.on_path());
    // Renamed: the old script goes, the new one comes, `ferrum list` follows.
    ops::set_command(&env, "editor", "ide", true).expect("rename");
    assert!(read(bin.join("fei")).is_none());
    assert!(read(bin.join("ide")).is_some());
    assert!(
        read(bin.join("ferrum"))
            .expect("ferrum")
            .contains(" ide \"$s\"")
    );
    // Clashes are refused and change nothing.
    ops::set_command(&env, "git", "", true).expect("git");
    let error = ops::set_command(&env, "git", "ide", true).expect_err("refused");
    assert!(error.contains("FerrumIDE"), "{error}");
    assert!(read(bin.join("feg")).is_some());
    // Off keeps the name for later.
    ops::set_command(&env, "editor", "", false).expect("off");
    assert!(read(bin.join("ide")).is_none());
    let prefs = SharedPrefs::load(&env.prefs_path());
    assert_eq!(prefs.command("editor").name, "ide");
    assert!(!prefs.command("editor").enabled);
    // Renaming while off checks the name but writes no script.
    ops::set_command(&env, "editor", "myide", false).expect("rename off");
    assert!(read(bin.join("myide")).is_none());
    assert!(ops::set_command(&env, "editor", "My IDE", false).is_err());
    // The umbrella can be turned off and on.
    ops::set_umbrella(&env, false).expect("off");
    assert!(read(bin.join("ferrum")).is_none());
    ops::set_umbrella(&env, true).expect("on");
    assert!(read(bin.join("ferrum")).is_some());
    // The Dock choice lives in the same file.
    ops::set_hidden(&env.prefs_path(), "ferrumgit", true).expect("hide");
    let prefs = SharedPrefs::load(&env.prefs_path());
    assert!(prefs.hide_from_dock.contains("ferrumgit"));
    assert_eq!(prefs.enabled_command("git").as_deref(), Some("feg"));
}

#[cfg(unix)]
#[test]
fn a_foreign_file_turns_the_command_off() {
    let dir = tempfile::tempdir().expect("tmp");
    let env = env(dir.path());
    std::fs::create_dir_all(env.bin_dir()).expect("bin");
    // Written after the name was chosen.
    ops::set_command(&env, "term", "", true).expect("on");
    std::fs::remove_file(env.bin_dir().join("fet")).expect("rm");
    std::fs::write(env.bin_dir().join("fet"), "#!/bin/sh\necho mine\n").expect("mine");
    let mut prefs = SharedPrefs::load(&env.prefs_path());
    let synced = ops::sync(&env.bin_dir(), &mut prefs);
    assert_eq!(synced.turned_off.len(), 1);
    assert_eq!(prefs.enabled_command("term"), None);
}

#[test]
fn apps_take_over_the_scripts_of_an_older_launcher() {
    let dir = tempfile::tempdir().expect("tmp");
    let env = env(dir.path());
    let bin = env.bin_dir();
    std::fs::create_dir_all(&bin).expect("bin");
    let ide = ferrum_shell::apps::by_id("editor").expect("ide");
    let text = ferrum_shell::script::app_script(ide, "fee", &bin.join(ide.binary));
    std::fs::write(bin.join("fee"), text).expect("fee");
    let prefs = ops::load(&env.prefs_path(), &bin);
    assert_eq!(prefs.enabled_command("editor").as_deref(), Some("fei"));
    assert_eq!(prefs.commands_revision, COMMANDS_REVISION);
}

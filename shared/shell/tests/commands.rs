//! Names, scripts and PATH lines.

use std::path::Path;

use ferrum_shell::apps::{self, APPS};
use ferrum_shell::path::{Shell, ensure_on_path, is_on_path};
use ferrum_shell::script::{self, MARKER, app_script, conflict};
use ferrum_shell::{SharedPrefs, check, validate};

fn app(id: &str) -> &'static ferrum_shell::App {
    apps::by_id(id).expect("app")
}

#[test]
fn suggested_commands_follow_the_brand_letters_and_are_valid() {
    let expected = [
        ("editor", "fei"),
        ("vibe", "fen"),
        ("git", "feg"),
        ("term", "fet"),
        ("api", "fea"),
        ("dock", "fec"),
        ("data", "fed"),
        ("env", "fev"),
        ("ports", "fep"),
        ("kit", "fek"),
        ("system", "fes"),
        ("launcher", "fel"),
    ];
    for (id, command) in expected {
        assert_eq!(app(id).default_command, command, "{id}");
    }
    for app in &APPS {
        validate(app.default_command).expect("valid");
    }
}

#[test]
fn validation_takes_short_lowercase_names() {
    for good in ["fe", "fei", "my-ide", "ide2", "a2345678901234567890"] {
        assert!(validate(good).is_ok(), "{good}");
    }
    for bad in [
        "",
        "f",
        "Fee",
        "1x",
        "-x",
        "a b",
        "my_ide",
        "a23456789012345678901",
        "fé",
        "x/y",
    ] {
        assert!(validate(bad).is_err(), "{bad}");
    }
}

#[test]
fn check_refuses_clashes_reserved_names_and_foreign_files() {
    let dir = tempfile::tempdir().expect("tmp");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let mut prefs = SharedPrefs::default();
    prefs.set_command("git", "g", false);
    prefs.set_command("git", "gg", true);
    // Another app's chosen name, or its suggested one while it has none.
    assert!(
        check("editor", "gg", &prefs, &bin)
            .expect_err("refused")
            .contains("FerrumGIT")
    );
    assert!(
        check("editor", "fet", &prefs, &bin)
            .expect_err("refused")
            .contains("FerrumTerm")
    );
    // FerrumGIT's old suggestion is free now.
    assert!(check("editor", "feg", &prefs, &bin).is_ok());
    // Its own names are fine.
    assert!(check("editor", "fei", &prefs, &bin).is_ok());
    assert!(check("git", "gg", &prefs, &bin).is_ok());
    for reserved in ["ferrum", "ferrumeditor", "fterm"] {
        assert!(
            check("editor", reserved, &prefs, &bin).is_err(),
            "{reserved}"
        );
    }
    // Someone else's file in ~/.ferrum/bin; one of ours is fine.
    std::fs::write(bin.join("ide"), "#!/bin/sh\necho mine\n").expect("write");
    assert!(
        check("editor", "ide", &prefs, &bin)
            .expect_err("refused")
            .contains("wasn't made by Ferrum")
    );
    std::fs::write(bin.join("myide"), format!("#!/bin/sh\n{MARKER} for X\n")).expect("write");
    assert!(check("editor", "myide", &prefs, &bin).is_ok());
    assert!(check("nope", "nope", &prefs, &bin).is_err());
    assert!(check("editor", "Bad", &prefs, &bin).is_err());
}

#[test]
fn scripts_run_the_active_cli_link() {
    let text = app_script(
        app("editor"),
        "fei",
        Path::new("/Users/me/.ferrum/bin/ferrumeditor"),
    );
    assert!(text.starts_with("#!/bin/sh\n# Ferrum Launcher shell command for FerrumIDE"));
    assert!(text.contains("nohup '/Users/me/.ferrum/bin/ferrumeditor' \"$@\""));
    assert!(text.contains("fei: FerrumIDE is not installed"));
    let term = app_script(app("term"), "fet", Path::new("/b/ferrumterm"));
    assert!(term.contains("\"$fterm\" new \"$(basename \"$dir\")\" -c \"$dir\""));
    let dock = app_script(app("dock"), "fec", Path::new("/b/ferrumdock"));
    assert!(dock.contains("cli=\"$(dirname \"$real\")/ferrumdock-cli\""));
    assert!(dock.contains("exec \"$cli\" \"$@\""));
    assert!(dock.contains("nohup '/b/ferrumdock'"));
    let quoted = app_script(app("git"), "feg", Path::new("/it's/ferrumgit"));
    assert!(quoted.contains(r"'/it'\''s/ferrumgit'"));
}

#[cfg(unix)]
#[test]
fn write_runs_and_remove_only_touches_our_files() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().expect("tempdir");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("mkdir");
    // A fake app CLI that records its arguments and folder.
    let log = dir.path().join("ran.txt");
    let cli = bin.join("ferrumeditor");
    std::fs::write(
        &cli,
        format!("#!/bin/sh\necho \"$PWD $*\" > '{}'\n", log.display()),
    )
    .expect("write");
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let text = app_script(app("editor"), "fei", &cli);
    assert!(script::write(&bin, "fei", &text).expect("write"));
    assert!(
        !script::write(&bin, "fei", &text).expect("again"),
        "unchanged"
    );
    let work = dir.path().join("project");
    std::fs::create_dir_all(&work).expect("mkdir");
    let status = std::process::Command::new(bin.join("fei"))
        .arg(".")
        .current_dir(&work)
        .status()
        .expect("run");
    assert!(status.success());
    let ran = wait_for(&log);
    assert!(ran.trim_end().ends_with(" ."), "{ran}");
    assert!(ran.contains("project"), "{ran}");
    // Someone else's file of the same name is left alone.
    std::fs::write(bin.join("feg"), "#!/bin/sh\n").expect("write");
    assert!(script::write(&bin, "feg", "x").is_err());
    assert!(!script::remove(&bin, "feg").expect("remove"));
    assert!(bin.join("feg").exists());
    assert!(script::remove(&bin, "fei").expect("remove"));
    assert!(!bin.join("fei").exists());
    assert!(cli.exists(), "the CLI isn't ours");
}

pub fn wait_for(file: &Path) -> String {
    for _ in 0..1000 {
        if let Ok(text) = std::fs::read_to_string(file)
            && !text.is_empty()
        {
            return text;
        }
        std::thread::sleep(std::time::Duration::from_millis(20));
    }
    panic!("{} was never written", file.display());
}

#[cfg(unix)]
#[test]
fn sync_writes_wanted_and_removes_stale_scripts() {
    let dir = tempfile::tempdir().expect("tmp");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let ours = |name: &str| format!("#!/bin/sh\n{MARKER} for {name}\n");
    std::fs::write(bin.join("fee"), ours("FerrumIDE")).expect("fee");
    std::fs::write(bin.join("mine"), "#!/bin/sh\necho mine\n").expect("mine");
    std::fs::write(bin.join("taken"), "#!/bin/sh\necho mine\n").expect("taken");
    std::os::unix::fs::symlink(dir.path().join("app"), bin.join("ferrumeditor")).expect("link");
    let wanted = vec![
        ("fei".to_owned(), ours("FerrumIDE")),
        ("taken".to_owned(), ours("FerrumGIT")),
    ];
    let synced = script::sync(&bin, &wanted);
    assert!(synced.files_changed);
    assert_eq!(synced.failed.len(), 1);
    assert_eq!(synced.failed[0].0, "taken");
    assert!(!bin.join("fee").exists(), "stale script removed");
    assert!(bin.join("fei").exists());
    assert!(bin.join("mine").exists() && bin.join("ferrumeditor").is_symlink());
    assert_eq!(
        std::fs::read_to_string(bin.join("taken")).expect("taken"),
        "#!/bin/sh\necho mine\n"
    );
    let again = script::sync(&bin, &wanted[..1]);
    assert!(!again.files_changed && again.failed.is_empty());
}

#[test]
fn discover_finds_the_commands_by_their_app() {
    let dir = tempfile::tempdir().expect("tmp");
    let bin = dir.path();
    for (id, name) in [("editor", "fee"), ("git", "g")] {
        let entry = bin.join(app(id).binary);
        std::fs::write(bin.join(name), app_script(app(id), name, &entry)).expect("write");
    }
    std::fs::write(bin.join("mine"), "#!/bin/sh\n/x/ferrumgit'\n").expect("mine");
    assert_eq!(
        script::discover(bin),
        vec![("editor", "fee".to_owned()), ("git", "g".to_owned())]
    );
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
    assert!(!is_on_path(&bin, home.path(), Shell::Zsh));
    let changed = ensure_on_path(&bin, home.path(), Shell::Zsh).expect("zsh");
    assert_eq!(changed, vec![home.path().join(".zprofile")]);
    assert!(is_on_path(&bin, home.path(), Shell::Zsh));
    assert!(
        ensure_on_path(&bin, home.path(), Shell::Zsh)
            .expect("again")
            .is_empty()
    );
    std::fs::write(home.path().join(".bash_profile"), "alias ll='ls -l'").expect("write");
    ensure_on_path(&bin, home.path(), Shell::Bash).expect("bash");
    let bash = std::fs::read_to_string(home.path().join(".bash_profile")).expect("read");
    assert!(bash.starts_with("alias ll='ls -l'\n\n# Added by Ferrum Launcher"));
    ensure_on_path(&bin, home.path(), Shell::Fish).expect("fish");
    let fish =
        std::fs::read_to_string(home.path().join(".config/fish/conf.d/ferrum.fish")).expect("read");
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

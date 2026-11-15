//! `ferrum <app> [path]`.

use std::collections::HashSet;

use ferrum_shell::APPS;
use ferrum_shell::SharedPrefs;
use ferrum_shell::umbrella::{self, help, list_rows, lookup, words};

#[test]
fn every_app_has_a_word_and_no_word_opens_two_apps() {
    let expected = [
        ("ide", "editor"),
        ("engineer", "vibe"),
        ("git", "git"),
        ("term", "term"),
        ("api", "api"),
        ("containers", "dock"),
        ("data", "data"),
        ("variables", "env"),
        ("ports", "ports"),
        ("kit", "kit"),
        ("system", "system"),
        ("launcher", "launcher"),
    ];
    for (word, id) in expected {
        assert_eq!(lookup(word).map(|a| a.id), Some(id), "ferrum {word}");
    }
    // Ids and short commands work too.
    assert_eq!(lookup("editor").map(|a| a.alias), Some("ide"));
    assert_eq!(lookup("fec").map(|a| a.alias), Some("containers"));
    assert!(lookup("list").is_none() && lookup("help").is_none());
    let mut seen = HashSet::new();
    for app in &APPS {
        for word in words(app) {
            assert!(seen.insert(word), "`ferrum {word}` is ambiguous");
        }
    }
    for app in &APPS {
        assert!(help().contains(&format!("ferrum {}", app.alias)));
    }
}

#[test]
fn list_shows_the_short_commands_that_are_on() {
    let mut prefs = SharedPrefs::default();
    prefs.set_command("editor", "ide", true);
    prefs.set_command("git", "feg", false);
    let rows = list_rows(&prefs);
    assert_eq!(rows.len(), APPS.len());
    assert_eq!(rows[0], ("ide", "FerrumIDE", "ide".to_owned()));
    assert!(rows.iter().any(|r| r.0 == "git" && r.2 == "-"));
}

#[cfg(unix)]
#[test]
fn the_script_dispatches_to_each_app() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;
    let dir = tempfile::tempdir().expect("tmp");
    let bin = dir.path().join("bin");
    std::fs::create_dir_all(&bin).expect("bin");
    let log = dir.path().join("ran.txt");
    let cli = bin.join("ferrumgit");
    std::fs::write(
        &cli,
        format!("#!/bin/sh\necho \"$PWD $*\" > '{}'\n", log.display()),
    )
    .expect("cli");
    std::fs::set_permissions(&cli, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let script = bin.join(umbrella::NAME);
    std::fs::write(&script, umbrella::script(&bin, &SharedPrefs::default())).expect("write");
    std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o755)).expect("chmod");
    let run = |args: &[&str]| {
        Command::new(&script)
            .args(args)
            .current_dir(dir.path())
            .output()
            .expect("run")
    };
    let out = run(&["git", "repo", "--x"]);
    assert!(out.status.success(), "{out:?}");
    let ran = super_wait(&log);
    assert!(ran.trim_end().ends_with(" repo --x"), "{ran}");
    // An app that isn't installed, an unknown one, help and list.
    let missing = run(&["ide", "."]);
    assert_eq!(missing.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&missing.stderr).contains("ferrum ide: FerrumIDE is not installed")
    );
    let unknown = run(&["nope"]);
    assert_eq!(unknown.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("ferrum list"));
    for args in [&[][..], &["help"][..], &["--help"][..]] {
        let out = run(args);
        assert!(out.status.success());
        assert!(String::from_utf8_lossy(&out.stdout).starts_with("Usage: ferrum <app>"));
    }
    let list = String::from_utf8_lossy(&run(&["list"]).stdout).into_owned();
    assert!(
        list.lines()
            .any(|l| l.starts_with("git") && l.ends_with(" installed")),
        "{list}"
    );
    assert!(
        list.lines()
            .any(|l| l.starts_with("ide") && l.ends_with("not installed")),
        "{list}"
    );
    assert_eq!(list.lines().count(), APPS.len() + 1);
}

fn super_wait(file: &std::path::Path) -> String {
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

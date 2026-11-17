//! Runs the `fenv` binary against a temporary vault (cheap Argon2
//! parameters), with the password on stdin.

use std::io::Write;
use std::path::Path;
use std::process::{Command, Output, Stdio};

use ferrumenv_core::crypto::KdfParams;
use ferrumenv_core::store::Session;
use ferrumenv_core::{Variable, Vault};

const PASSWORD: &str = "correct horse battery";

fn make_vault(dir: &Path) -> std::path::PathBuf {
    let mut vault = Vault::default();
    let i = vault
        .add_project("shop", Some(dir.join("shop")))
        .expect("project");
    let dev = vault.projects[i].environment_mut("development");
    dev.upsert(Variable::new("API_KEY", "s3cret-api-key"));
    dev.upsert(Variable::new("GREETING", "hello world"));
    let path = dir.join("vault.json");
    Session::create(&path, PASSWORD, KdfParams::INSECURE_FAST, &vault).expect("vault");
    std::fs::create_dir_all(dir.join("shop/src")).expect("project folder");
    path
}

fn fenv(vault: &Path, cwd: &Path, password: &str, args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_fenv"))
        .arg("--vault")
        .arg(vault)
        .arg("--password-stdin")
        .args(args)
        .current_dir(cwd)
        .env("FENV_TEST_PARENT", "inherited")
        .env_remove("FENV_VAULT")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn fenv");
    child
        .stdin
        .take()
        .expect("stdin")
        .write_all(format!("{password}\n").as_bytes())
        .expect("password");
    child.wait_with_output().expect("fenv output")
}

fn stdout(output: &Output) -> String {
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn list_shows_names_and_values_only_when_asked() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = make_vault(dir.path());

    let projects = fenv(&vault, dir.path(), PASSWORD, &["list"]);
    assert!(projects.status.success(), "{}", stderr(&projects));
    assert!(stdout(&projects).contains("shop"));
    assert!(stdout(&projects).contains("development: 2 variables"));

    let names = fenv(
        &vault,
        dir.path(),
        PASSWORD,
        &["list", "-p", "shop", "-e", "dev"],
    );
    assert!(names.status.success(), "{}", stderr(&names));
    assert_eq!(stdout(&names), "API_KEY\nGREETING\n");

    // The project linked to the current folder is the default.
    let here = fenv(
        &vault,
        &dir.path().join("shop/src"),
        PASSWORD,
        &["list", "-e", "dev"],
    );
    assert_eq!(stdout(&here), "API_KEY\nGREETING\n");

    let shown = fenv(
        &vault,
        dir.path(),
        PASSWORD,
        &["list", "-p", "shop", "-e", "dev", "--show"],
    );
    assert!(stdout(&shown).contains("API_KEY=s3cret-api-key"));
    assert!(stdout(&shown).contains("GREETING='hello world'"));

    for output in [&projects, &names, &here] {
        assert!(!stdout(output).contains("s3cret"));
        assert!(!stderr(output).contains("s3cret"));
    }
}

#[test]
fn wrong_password_and_unknown_names_fail() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = make_vault(dir.path());
    let wrong = fenv(&vault, dir.path(), "not the password", &["list"]);
    assert!(!wrong.status.success());
    assert!(stderr(&wrong).contains("Wrong master password"));
    assert!(stdout(&wrong).is_empty());

    let missing = fenv(&vault, dir.path(), PASSWORD, &["list", "-p", "nope"]);
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("no project named nope"));

    let no_vault = fenv(
        &dir.path().join("none.json"),
        dir.path(),
        PASSWORD,
        &["list"],
    );
    assert!(stderr(&no_vault).contains("no vault at"));
}

#[cfg(unix)]
#[test]
fn run_injects_variables_without_writing_files() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = make_vault(dir.path());
    let work = dir.path().join("shop");
    let before: Vec<_> = std::fs::read_dir(&work).expect("list").collect();
    let output = fenv(
        &vault,
        &work,
        PASSWORD,
        &[
            "run",
            "--env",
            "dev",
            "--",
            "sh",
            "-c",
            "printf '%s|%s|%s' \"$API_KEY\" \"$GREETING\" \"$FENV_TEST_PARENT\"; exit 3",
        ],
    );
    assert_eq!(stdout(&output), "s3cret-api-key|hello world|inherited");
    // The child's exit status is passed through (exec).
    assert_eq!(output.status.code(), Some(3));
    assert!(stderr(&output).contains("2 variables from shop › development"));
    assert!(!stderr(&output).contains("s3cret"));
    let after: Vec<_> = std::fs::read_dir(&work).expect("list").collect();
    assert_eq!(before.len(), after.len(), "no .env file written");

    let missing = fenv(
        &vault,
        &work,
        PASSWORD,
        &["run", "--", "no-such-program-fenv"],
    );
    assert!(!missing.status.success());
    assert!(stderr(&missing).contains("could not run no-such-program-fenv"));
}

fn git(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(["-c", "user.name=t", "-c", "user.email=t@example.com"])
        .args(["-c", "commit.gpgsign=false"])
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[test]
fn scan_reports_leaks_by_name_only() {
    let dir = tempfile::tempdir().expect("tempdir");
    let vault = make_vault(dir.path());
    let work = dir.path().join("shop");
    let clean = fenv(&vault, &work, PASSWORD, &["scan"]);
    assert!(clean.status.success(), "{}", stderr(&clean));
    assert!(stdout(&clean).contains("No stored secret values found"));

    if !git(&work, &["init", "-q"]) {
        return;
    }
    std::fs::write(work.join("src/settings.py"), "KEY = 's3cret-api-key'\n").expect("write");
    assert!(git(&work, &["add", "."]));
    assert!(git(&work, &["commit", "-q", "-m", "oops"]));
    std::fs::write(work.join("src/settings.py"), "KEY = env('KEY')\n").expect("write");
    assert!(git(&work, &["commit", "-q", "-am", "fix"]));
    std::fs::write(work.join("notes.txt"), "\n\ntoken s3cret-api-key\n").expect("write");

    let output = fenv(&vault, &work, PASSWORD, &["scan"]);
    assert_eq!(output.status.code(), Some(1));
    let text = stdout(&output);
    assert!(
        text.contains("notes.txt:3  API_KEY (development)"),
        "{text}"
    );
    assert!(text.contains("src/settings.py (history, commit "), "{text}");
    assert!(text.contains("rotate it"));
    assert!(!text.contains("s3cret") && !stderr(&output).contains("s3cret"));

    let tree_only = fenv(&vault, &work, PASSWORD, &["scan", "--no-history"]);
    assert!(!stdout(&tree_only).contains("(history, commit"));
    assert!(stdout(&tree_only).contains("notes.txt:3"));
}

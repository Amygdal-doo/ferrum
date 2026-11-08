//! Per-profile SSH environment and the askpass helper.
//!
//! Network commands get:
//! - `GIT_SSH_COMMAND="ssh -i '<key>' -o IdentitiesOnly=yes"` when the
//!   profile pins a key, so this repository authenticates with that key only;
//! - `SSH_ASKPASS=<FerrumGIT binary>` + `SSH_ASKPASS_REQUIRE=force`, so ssh
//!   never prompts on a terminal. If the profile has a stored passphrase it is
//!   written to a 0600 temp file that lives only for the duration of the
//!   command; the helper prints it for passphrase prompts and refuses others.

use crate::error::{GitError, GitResult, sanitize};
use std::ffi::OsString;
use std::io::Write;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio::process::Command;

/// Set when the FerrumGIT binary is invoked as an askpass helper.
pub const ASKPASS_ENV: &str = "FERRUMGIT_ASKPASS";
/// Path of the temporary passphrase file.
pub const ASKPASS_FILE_ENV: &str = "FERRUMGIT_ASKPASS_FILE";

/// Environment for one network command. Holds the passphrase file alive.
#[derive(Debug, Clone, Default)]
pub struct NetEnv {
    vars: Vec<(String, OsString)>,
    _passphrase: Option<Arc<tempfile::NamedTempFile>>,
}

impl NetEnv {
    /// `key`: private key to force; `None` = system ssh behaviour.
    /// `askpass`: program to answer prompts (normally `current_exe`).
    pub fn new(
        key: Option<&str>,
        passphrase: Option<&str>,
        askpass: Option<&Path>,
    ) -> GitResult<Self> {
        let mut vars = Vec::new();
        if let Some(key) = key {
            let expanded = expand_home(key);
            if !Path::new(&expanded).is_file() {
                return Err(GitError::Invalid(format!("SSH key not found: {expanded}")));
            }
            vars.push((
                "GIT_SSH_COMMAND".to_owned(),
                ssh_command(Some(&expanded), &[]).into(),
            ));
        }
        let mut file = None;
        if let Some(program) = askpass {
            vars.push(("SSH_ASKPASS".to_owned(), program.as_os_str().to_owned()));
            vars.push(("SSH_ASKPASS_REQUIRE".to_owned(), "force".into()));
            // Older OpenSSH only uses askpass when DISPLAY is set.
            vars.push((
                "DISPLAY".to_owned(),
                std::env::var_os("DISPLAY").unwrap_or_else(|| ":0".into()),
            ));
            vars.push((ASKPASS_ENV.to_owned(), "1".into()));
            if let Some(p) = passphrase.filter(|p| !p.is_empty()) {
                let mut f = tempfile::Builder::new()
                    .prefix("ferrumgit-askpass-")
                    .tempfile()
                    .map_err(|e| GitError::Spawn(format!("passphrase file: {e}")))?;
                f.write_all(p.as_bytes())
                    .and_then(|()| f.flush())
                    .map_err(|e| GitError::Spawn(format!("passphrase file: {e}")))?;
                vars.push((ASKPASS_FILE_ENV.to_owned(), f.path().as_os_str().to_owned()));
                file = Some(Arc::new(f));
            }
        }
        Ok(Self {
            vars,
            _passphrase: file,
        })
    }

    #[must_use]
    pub fn vars(&self) -> &[(String, OsString)] {
        &self.vars
    }
}

/// `ssh` command line for `GIT_SSH_COMMAND` (interpreted by `sh`).
#[must_use]
pub fn ssh_command(key: Option<&str>, extra: &[&str]) -> String {
    let mut cmd = String::from("ssh");
    if let Some(k) = key {
        cmd.push_str(" -i ");
        cmd.push_str(&shell_quote(k));
        cmd.push_str(" -o IdentitiesOnly=yes");
    }
    for e in extra {
        cmd.push(' ');
        cmd.push_str(e);
    }
    cmd
}

fn shell_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"'\''"))
}

/// `~/x` → `$HOME/x`.
#[must_use]
pub fn expand_home(path: &str) -> String {
    match (path.strip_prefix("~/"), std::env::var_os("HOME")) {
        (Some(rest), Some(home)) => Path::new(&home).join(rest).display().to_string(),
        _ => path.to_owned(),
    }
}

/// Entry point when the binary runs as `SSH_ASKPASS`. Returns the exit code
/// to use, or `None` for a normal app launch. Call first thing in `main`.
#[must_use]
pub fn askpass_main() -> Option<i32> {
    std::env::var_os(ASKPASS_ENV)?;
    let prompt = std::env::args().nth(1).unwrap_or_default().to_lowercase();
    let file = std::env::var_os(ASKPASS_FILE_ENV);
    if prompt.contains("passphrase") || prompt.contains("pin for") {
        if let Some(path) = file
            && let Ok(secret) = std::fs::read_to_string(path)
        {
            println!("{secret}");
            return Some(0);
        }
        eprintln!("FerrumGIT: this key needs a passphrase; store it in the repository's profile");
        return Some(1);
    }
    eprintln!("FerrumGIT: declined interactive prompt");
    Some(1)
}

/// `ssh -T git@host` with the profile's key. Trusts a new host key on first
/// use (`accept-new`), so later git commands can connect. Returns the server
/// greeting on success.
pub async fn test_connection(
    host: &str,
    user: &str,
    key: Option<&str>,
    net: &NetEnv,
) -> GitResult<String> {
    let host = host.trim();
    if host.is_empty() || host.starts_with('-') || host.contains(char::is_whitespace) {
        return Err(GitError::Invalid("enter a host such as github.com".into()));
    }
    let mut cmd = Command::new("ssh");
    cmd.arg("-T").args([
        "-o",
        "StrictHostKeyChecking=accept-new",
        "-o",
        "ConnectTimeout=15",
    ]);
    if let Some(k) = key {
        cmd.arg("-i")
            .arg(expand_home(k))
            .args(["-o", "IdentitiesOnly=yes"]);
    }
    cmd.arg(format!("{user}@{host}"))
        .stdin(std::process::Stdio::null())
        .kill_on_drop(true);
    for (k, v) in net.vars() {
        if k != "GIT_SSH_COMMAND" {
            cmd.env(k, v);
        }
    }
    let out = tokio::time::timeout(Duration::from_secs(30), cmd.output())
        .await
        .map_err(|_| GitError::Invalid("connection timed out".into()))?
        .map_err(|e| GitError::Spawn(format!("ssh: {e}")))?;
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let text = sanitize(text.trim());
    // GitHub exits 1 with "successfully authenticated"; GitLab/Gitea exit 0.
    let ok = out.status.success()
        || [
            "successfully authenticated",
            "welcome to gitlab",
            "authenticated via",
            "you've successfully",
        ]
        .iter()
        .any(|s| text.to_lowercase().contains(s));
    if ok {
        Ok(text
            .lines()
            .find(|l| !l.starts_with("Warning:"))
            .unwrap_or(&text)
            .to_owned())
    } else {
        Err(GitError::Failed {
            code: out.status.code().unwrap_or(-1),
            message: text,
        }
        .with_ssh_hint())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_command_forces_identity() {
        assert_eq!(
            ssh_command(Some("/Users/me/.ssh/work key"), &[]),
            "ssh -i '/Users/me/.ssh/work key' -o IdentitiesOnly=yes"
        );
        assert_eq!(
            ssh_command(Some("/a/it's"), &[]),
            r"ssh -i '/a/it'\''s' -o IdentitiesOnly=yes"
        );
        assert_eq!(ssh_command(None, &[]), "ssh");
    }

    #[test]
    fn missing_key_is_reported() {
        let r = NetEnv::new(Some("/definitely/not/here"), None, None);
        assert!(matches!(r, Err(GitError::Invalid(m)) if m.contains("SSH key not found")));
    }

    #[test]
    fn passphrase_file_is_private_and_removed() {
        let env = NetEnv::new(None, Some("s3cret"), Some(Path::new("/bin/echo"))).expect("env");
        let path = env
            .vars()
            .iter()
            .find(|(k, _)| k == ASKPASS_FILE_ENV)
            .map(|(_, v)| std::path::PathBuf::from(v))
            .expect("file var");
        assert_eq!(std::fs::read_to_string(&path).expect("read"), "s3cret");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = std::fs::metadata(&path).expect("meta").permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }
        drop(env);
        assert!(!path.exists());
    }

    #[test]
    fn no_passphrase_without_askpass() {
        let env = NetEnv::new(None, Some("x"), None).expect("env");
        assert!(env.vars().is_empty());
    }
}

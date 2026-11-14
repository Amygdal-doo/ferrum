//! The login shell's `PATH`. Apps started from the Dock or Finder get a
//! minimal one, so `npx`, `gemini` or MCP servers would not be found.

use std::io::Read as _;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

const MARKER: &str = "__FERRUM_PATH__";

/// Runs `$SHELL -l -i -c` to print `PATH`; `None` on Windows, without a
/// shell, or after 4 seconds.
#[must_use]
pub fn login_shell_path() -> Option<String> {
    if cfg!(windows) {
        return None;
    }
    let shell = std::env::var("SHELL").ok().filter(|s| !s.is_empty())?;
    let mut child = Command::new(shell)
        .args([
            "-l",
            "-i",
            "-c",
            &format!("printf '\\n{MARKER}%s\\n' \"$PATH\""),
        ])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + Duration::from_secs(4);
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(20)),
            _ => {
                child.kill().ok();
                child.wait().ok();
                return None;
            }
        }
    }
    let mut out = String::new();
    child.stdout.take()?.read_to_string(&mut out).ok()?;
    parse(&out)
}

fn parse(output: &str) -> Option<String> {
    output
        .lines()
        .find_map(|line| line.strip_prefix(MARKER))
        .map(str::to_string)
        .filter(|p| !p.is_empty())
}

#[cfg(test)]
mod tests {
    #[test]
    fn finds_the_marked_line_among_shell_noise() {
        let out = "welcome!\n\n__FERRUM_PATH__/opt/homebrew/bin:/usr/bin\n";
        assert_eq!(
            super::parse(out).as_deref(),
            Some("/opt/homebrew/bin:/usr/bin")
        );
        assert_eq!(super::parse("no marker"), None);
        assert_eq!(super::parse("__FERRUM_PATH__"), None);
    }
}

//! OS integration: keep-awake while agents work, and notifications.

use ferrumterm_protocol::{AwakeMode, BlockId, ProgramState, ProgramStatus};
use std::process::{Child, Command, Stdio};

/// Holds `caffeinate -i -s -w <server pid>` on macOS: prevents idle sleep
/// (and system sleep on AC power) while agents work, so they keep running
/// with the display off. It cannot keep a laptop awake with the lid closed
/// unless an external display and power are connected (clamshell mode).
pub struct Awake {
    pub mode: AwakeMode,
    child: Option<Child>,
}

impl Awake {
    pub fn new(mode: AwakeMode) -> Self {
        Self { mode, child: None }
    }

    pub fn active(&self) -> bool {
        self.child.is_some()
    }

    pub fn update(&mut self, working: bool) {
        let want = match self.mode {
            AwakeMode::Always => true,
            AwakeMode::Never => false,
            AwakeMode::Auto => working,
        };
        if let Some(c) = self.child.as_mut()
            && c.try_wait().ok().flatten().is_some()
        {
            self.child = None;
        }
        match (want, self.child.take()) {
            (true, Some(c)) => self.child = Some(c),
            (true, None) if cfg!(target_os = "macos") => {
                let spawned = Command::new("/usr/bin/caffeinate")
                    .args(["-i", "-s", "-w", &std::process::id().to_string()])
                    .stdin(Stdio::null())
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .spawn();
                match spawned {
                    Ok(c) => {
                        tracing::info!("keep-awake on");
                        self.child = Some(c);
                    }
                    Err(e) => tracing::warn!(error = %e, "could not start caffeinate"),
                }
            }
            (true, None) => {}
            (false, Some(mut c)) => {
                let _ = c.kill();
                let _ = c.wait();
                tracing::info!("keep-awake off");
            }
            (false, None) => {}
        }
    }
}

impl Drop for Awake {
    fn drop(&mut self) {
        self.update(false);
    }
}

fn applescript_string(s: &str) -> String {
    let clean: String = s.chars().filter(|c| !c.is_control()).take(200).collect();
    format!("\"{}\"", clean.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Desktop notification for a status change (macOS only).
pub fn notify(block: BlockId, label: &str, status: &ProgramStatus) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let headline = match status.state {
        ProgramState::Done => "finished",
        ProgramState::Blocked => match status.kind.as_deref() {
            Some("permission") => "needs permission",
            Some("question") => "has a question",
            Some("auth") => "needs to sign in",
            _ => "is waiting for you",
        },
        ProgramState::Error => "failed",
        _ => return,
    };
    let name = if label.trim().is_empty() {
        block.to_string()
    } else {
        format!("{label} ({block})")
    };
    let body = status
        .message
        .clone()
        .unwrap_or_else(|| format!("{name} {headline}"));
    let script = format!(
        "display notification {} with title {} subtitle {}",
        applescript_string(&body),
        applescript_string("FerrumTerm"),
        applescript_string(&format!("{name} {headline}")),
    );
    let _ = Command::new("/usr/bin/osascript")
        .args(["-e", &script])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn();
}

#[cfg(test)]
mod tests {
    use super::applescript_string;

    #[test]
    fn escapes_applescript() {
        assert_eq!(
            applescript_string(r#"say "hi" \ now"#),
            r#""say \"hi\" \\ now""#
        );
        assert_eq!(applescript_string("a\nb"), "\"ab\"");
    }
}

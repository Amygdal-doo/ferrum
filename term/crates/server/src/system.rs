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

/// Whether Settings › Notifications in FerrumTerm allows a system
/// notification for `event` (`~/.ferrum/ferrumterm/notifications.json`,
/// written by `ferrum_ui::notify`). Missing settings allow it.
fn system_notifications_allowed(event: &str) -> bool {
    let dir = match std::env::var_os("FERRUM_HOME") {
        Some(dir) => std::path::PathBuf::from(dir),
        None => match std::env::var_os("HOME") {
            Some(home) => std::path::Path::new(&home).join(".ferrum"),
            None => return true,
        },
    };
    let Ok(text) = std::fs::read_to_string(dir.join("ferrumterm/notifications.json")) else {
        return true;
    };
    allowed_in(&text, event)
}

fn allowed_in(text: &str, event: &str) -> bool {
    let Ok(prefs) = serde_json::from_str::<serde_json::Value>(text) else {
        return true;
    };
    let on = |v: Option<&serde_json::Value>| v.and_then(serde_json::Value::as_bool) != Some(false);
    on(prefs.get("system")) && on(prefs.pointer(&format!("/events/{event}/system")))
}

/// Desktop notification for a status change (macOS only).
pub fn notify(block: BlockId, label: &str, status: &ProgramStatus) {
    if !cfg!(target_os = "macos") {
        return;
    }
    let event = match status.state {
        ProgramState::Done => "done",
        ProgramState::Blocked => "blocked",
        ProgramState::Error => "error",
        _ => return,
    };
    if !system_notifications_allowed(event) {
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

#[cfg(test)]
mod notification_pref_tests {
    use super::allowed_in;

    #[test]
    fn honours_master_and_event_switches() {
        assert!(allowed_in("{}", "done"));
        assert!(allowed_in("not json", "done"));
        assert!(!allowed_in(r#"{"system": false}"#, "done"));
        let per_event = r#"{"system": true, "events": {"done": {"system": false}}}"#;
        assert!(!allowed_in(per_event, "done"));
        assert!(allowed_in(per_event, "blocked"));
    }
}

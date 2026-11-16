//! The bits of the operating system sign-in needs: opening the browser and
//! a human-readable device name for the consent page.

use std::process::{Command, Stdio};

use crate::{AccountError, Result};

/// Opens `url` in the default browser (`open` on macOS, `xdg-open` on
/// Linux, `explorer` on Windows).
pub fn open_browser(url: &str) -> Result<()> {
    let program = if cfg!(target_os = "macos") {
        "open"
    } else if cfg!(windows) {
        "explorer"
    } else {
        "xdg-open"
    };
    Command::new(program)
        .arg(url)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map(|_| ())
        .map_err(|e| AccountError::Browser(format!("{program}: {e}")))
}

/// "Ada's MacBook Pro" on macOS (the Sharing name), else the host name,
/// else "This computer".
pub async fn device_name() -> String {
    let mut candidates: Vec<(&str, &[&str])> = Vec::new();
    if cfg!(target_os = "macos") {
        candidates.push(("scutil", &["--get", "ComputerName"]));
    }
    candidates.push(("hostname", &[]));
    for (program, args) in candidates {
        let output = tokio::process::Command::new(program)
            .args(args)
            .stdin(Stdio::null())
            .stderr(Stdio::null())
            .output()
            .await;
        if let Ok(output) = output
            && output.status.success()
            && let Ok(name) = String::from_utf8(output.stdout)
        {
            let name = name.trim();
            if !name.is_empty() {
                return name.chars().take(100).collect();
            }
        }
    }
    "This computer".into()
}

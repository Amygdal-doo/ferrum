//! The `git:*` commands: ⌥⌘G / ⌃⇧G toggle the Git pane (FerrumGIT's view,
//! embedded); "Open in FerrumGIT app" launches the stand-alone app on the
//! project.

use std::path::Path;

use ferrumeditor_core::tool_layout::ToolKind;

use super::find_on_path;
use crate::registry::CommandRegistry;

pub(super) fn register(r: &mut CommandRegistry) {
    for name in ["git:toggle-view", "git:show-panel"] {
        r.register(name, |w, window, cx| {
            w.toggle_tool_focus(ToolKind::Git, window, cx)
        });
    }
    r.register("git:open-in-ferrumgit", |w, _, cx| {
        let Some(root) = w.root.clone() else {
            w.info("Open a folder first", cx);
            return;
        };
        match open_in_ferrumgit(&root) {
            Ok(()) => w.info("Opened in FerrumGIT", cx),
            Err(error) => w.error(error, cx),
        }
    });
}

/// Starts the FerrumGIT app on `repo`: the `ferrumgit` binary on PATH,
/// else (macOS) the installed app bundle.
fn open_in_ferrumgit(repo: &Path) -> Result<(), String> {
    if let Some(binary) = find_on_path("ferrumgit") {
        return std::process::Command::new(binary)
            .arg(repo)
            .current_dir(repo)
            .spawn()
            .map(drop)
            .map_err(|e| format!("Could not open FerrumGIT: {e}"));
    }
    if cfg!(target_os = "macos") {
        let status = std::process::Command::new("open")
            .args(["-a", "FerrumGIT"])
            .arg(repo)
            .status()
            .map_err(|e| format!("Could not open FerrumGIT: {e}"))?;
        if status.success() {
            return Ok(());
        }
    }
    Err("FerrumGIT is not installed (no `ferrumgit` on PATH)".into())
}

//! The shared Ferrum project bar above the window: the project (git
//! repository, else folder) of the focused pane's working directory.
//!
//! The server only knows the directory a block was started in, so the
//! live one (after `cd`) is read from the block's process: `/proc/<pid>/cwd`
//! on Linux, `lsof` on macOS. That runs off the UI thread, at most once a
//! second, after the focused pane's screen changed (a `cd` always redraws
//! the prompt), so an idle window does no work.

use crate::app::{App, spawn_env};
use crate::view::TermView;
use ferrum_ui::gpui::{AppContext as _, Context, Entity, Subscription, Task, Window};
use ferrum_ui::project::{project_name, project_root};
use ferrum_ui::{ProjectBar, ProjectBarEvent};
use ferrumterm_protocol::{BlockId, Request, Spawn, WindowInfo};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;

/// The executable name recorded in `~/.ferrum/projects.json`.
const APP_BINARY: &str = "ferrumterm";
/// Shortest gap between two working-directory reads.
const CWD_CHECK_DELAY: Duration = Duration::from_millis(1000);

pub struct ProjectState {
    pub bar: Entity<ProjectBar>,
    /// Working directories read from the blocks' processes.
    live: HashMap<BlockId, PathBuf>,
    /// The focused block and directory the bar was last set from.
    shown: Option<(BlockId, PathBuf)>,
    check: Option<Task<()>>,
    _events: Subscription,
}

impl ProjectState {
    pub fn new(window: &mut Window, cx: &mut Context<TermView>) -> Self {
        let bar = cx.new(|cx| ProjectBar::new(APP_BINARY, window, cx).inline(true));
        let events = cx.subscribe(&bar, |view: &mut TermView, _, event, cx| match event {
            ProjectBarEvent::Open(path) => {
                view.app.open_folder(path);
                view.after_change(cx);
            }
        });
        Self {
            bar,
            live: HashMap::new(),
            shown: None,
            check: None,
            _events: events,
        }
    }
}

/// The focused block of `window` and its working directory: the live one
/// when known, else the one it was started in, else home (the server's
/// default).
#[must_use]
pub fn focused_dir(
    window: &WindowInfo,
    live: &HashMap<BlockId, PathBuf>,
    home: Option<&Path>,
) -> Option<(BlockId, PathBuf)> {
    let block = window.blocks.iter().find(|b| b.id == window.focused)?;
    let dir = live
        .get(&block.id)
        .cloned()
        .or_else(|| block.cwd.as_ref().map(PathBuf::from))
        .or_else(|| home.map(Path::to_path_buf))?;
    Some((block.id, dir))
}

/// A new tab in `session` (a new session when there is none) running a
/// shell in `folder`, named after it.
#[must_use]
pub fn open_request(session: Option<String>, folder: &Path) -> Request {
    let name = Some(project_name(folder));
    let spawn = Spawn {
        cwd: Some(folder.display().to_string()),
        ..spawn_env()
    };
    match session {
        Some(session) => Request::NewWindow {
            session: Some(session),
            name,
            spawn,
        },
        None => Request::NewSession { name, spawn },
    }
}

/// `lsof -Fn` output: the `n` line is the path.
fn parse_lsof(output: &str) -> Option<PathBuf> {
    output
        .lines()
        .find_map(|line| line.strip_prefix('n'))
        .filter(|path| path.starts_with('/'))
        .map(PathBuf::from)
}

/// The current working directory of process `pid`.
fn process_cwd(pid: u32) -> Option<PathBuf> {
    if cfg!(target_os = "linux") {
        return std::fs::read_link(format!("/proc/{pid}/cwd")).ok();
    }
    let lsof = if Path::new("/usr/sbin/lsof").is_file() {
        "/usr/sbin/lsof"
    } else {
        "lsof"
    };
    let output = std::process::Command::new(lsof)
        .args(["-a", "-p", &pid.to_string(), "-d", "cwd", "-Fn"])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .output()
        .ok()?;
    parse_lsof(&String::from_utf8_lossy(&output.stdout))
}

impl App {
    /// Opens a new tab in `folder` (the project bar's switcher).
    pub fn open_folder(&mut self, folder: &Path) {
        if self.client.is_none() {
            self.notify("Not connected to the FerrumTerm server yet");
            return;
        }
        let session = self.session().map(|s| s.id.to_string());
        self.send(open_request(session, folder));
    }
}

impl TermView {
    /// Points the bar at the focused pane's project; reads the pane's live
    /// directory when another pane got focus.
    pub(crate) fn sync_project(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.project.as_mut() else {
            return;
        };
        let home = ferrum_ui::project::home_dir();
        let focused = self
            .app
            .window()
            .and_then(|w| focused_dir(w, &state.live, home.as_deref()));
        if focused == state.shown {
            return;
        }
        let new_block = focused.as_ref().map(|f| f.0) != state.shown.as_ref().map(|f| f.0);
        let project = focused.as_ref().map(|(_, dir)| project_root(dir));
        state.shown = focused;
        state.bar.update(cx, |bar, cx| bar.set_project(project, cx));
        if new_block {
            self.check_cwd_soon(cx);
        }
    }

    /// The focused pane's screen changed: it may have changed directory.
    pub(crate) fn focused_frame(&mut self, block: BlockId, cx: &mut Context<Self>) {
        if self.project.is_some() && Some(block) == self.app.focused() {
            self.check_cwd_soon(cx);
        }
    }

    fn check_cwd_soon(&mut self, cx: &mut Context<Self>) {
        let Some(state) = self.project.as_mut() else {
            return;
        };
        if state.check.is_some() {
            return;
        }
        state.check = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(CWD_CHECK_DELAY).await;
            let Ok(target) = this.update(cx, |view, _| {
                let window = view.app.window()?;
                let block = window.blocks.iter().find(|b| b.id == window.focused)?;
                Some((block.id, block.pid?))
            }) else {
                return;
            };
            let cwd = match target {
                Some((_, pid)) => cx.background_spawn(async move { process_cwd(pid) }).await,
                None => None,
            };
            let updated = this.update(cx, |view, cx| {
                let open: Vec<BlockId> = view
                    .app
                    .sessions
                    .iter()
                    .flat_map(|s| &s.windows)
                    .flat_map(|w| w.blocks.iter().map(|b| b.id))
                    .collect();
                let Some(state) = view.project.as_mut() else {
                    return;
                };
                state.check = None;
                if let (Some((block, _)), Some(cwd)) = (target, cwd) {
                    // Forget closed blocks.
                    state.live.retain(|b, _| open.contains(b));
                    state.live.insert(block, cwd);
                    view.sync_project(cx);
                }
            });
            if let Err(error) = updated {
                tracing::debug!(%error, "window closed before a directory check finished");
            }
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferrumterm_protocol::{BlockInfo, Layout, WindowId};

    fn block(id: u32, cwd: Option<&str>) -> BlockInfo {
        BlockInfo {
            id: BlockId(id),
            label: None,
            title: String::new(),
            command: Vec::new(),
            cwd: cwd.map(str::to_owned),
            pid: Some(100 + id),
            cols: 80,
            rows: 24,
            status: None,
            exited: None,
        }
    }

    fn window(focused: u32, blocks: Vec<BlockInfo>) -> WindowInfo {
        WindowInfo {
            id: WindowId(1),
            name: "1".into(),
            layout: Layout::Block(BlockId(focused)),
            focused: BlockId(focused),
            zoomed: false,
            cols: 80,
            rows: 24,
            blocks,
        }
    }

    #[test]
    fn the_focused_pane_decides_the_directory() {
        let w = window(
            2,
            vec![block(1, Some("/code/a")), block(2, Some("/code/b"))],
        );
        let mut live = HashMap::new();
        let home = Path::new("/Users/me");
        assert_eq!(
            focused_dir(&w, &live, Some(home)),
            Some((BlockId(2), "/code/b".into()))
        );
        // After `cd`, the live directory wins over the start directory.
        live.insert(BlockId(2), PathBuf::from("/code/b/src"));
        assert_eq!(
            focused_dir(&w, &live, Some(home)),
            Some((BlockId(2), "/code/b/src".into()))
        );
        // Started without a directory: the server used home.
        let w = window(3, vec![block(3, None)]);
        assert_eq!(
            focused_dir(&w, &live, Some(home)),
            Some((BlockId(3), home.into()))
        );
        assert_eq!(focused_dir(&w, &live, None), None);
    }

    #[test]
    fn the_project_is_the_repository_around_the_directory() {
        let dir = std::env::temp_dir().join(format!("ferrumterm-project-{}", std::process::id()));
        let deep = dir.join("repo/src/deep");
        std::fs::create_dir_all(&deep).expect("mkdir");
        std::fs::create_dir_all(dir.join("repo/.git")).expect("mkdir");
        let w = window(1, vec![block(1, Some(&deep.display().to_string()))]);
        let (_, cwd) = focused_dir(&w, &HashMap::new(), None).expect("focused");
        assert_eq!(project_root(&cwd), dir.join("repo"));
        std::fs::remove_dir_all(&dir).expect("cleanup");
    }

    #[test]
    fn opening_a_folder_starts_a_tab_there() {
        let folder = Path::new("/code/app");
        match open_request(Some("s1".into()), folder) {
            Request::NewWindow {
                session,
                name,
                spawn,
            } => {
                assert_eq!(session.as_deref(), Some("s1"));
                assert_eq!(name.as_deref(), Some("app"));
                assert_eq!(spawn.cwd.as_deref(), Some("/code/app"));
                assert!(spawn.command.is_empty(), "a login shell");
            }
            other => panic!("unexpected request {other:?}"),
        }
        assert!(matches!(
            open_request(None, folder),
            Request::NewSession {
                spawn: Spawn { cwd: Some(_), .. },
                ..
            }
        ));
    }

    #[test]
    fn lsof_output_gives_the_path() {
        assert_eq!(
            parse_lsof("p123\nfcwd\nn/Users/me/code\n"),
            Some("/Users/me/code".into())
        );
        assert_eq!(parse_lsof(""), None);
    }

    #[test]
    fn this_process_has_a_readable_directory() {
        let me = std::process::id();
        let cwd = std::env::current_dir().expect("cwd");
        let found = process_cwd(me).expect("lsof or /proc");
        assert_eq!(
            std::fs::canonicalize(found).expect("canonical"),
            std::fs::canonicalize(cwd).expect("canonical")
        );
    }
}

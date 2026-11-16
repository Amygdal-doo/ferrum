//! The embedding surface: view options, events for the host, key bindings
//! and the process-wide background services.

use crate::app::{GoBack, GoForward, KEY_CONTEXT, PICKER_CONTEXT, QuickOpen, Reload};
use ferrumgit_application::AppServices;
use gpui::{App, Global, KeyBinding};
use std::path::PathBuf;

/// How a [`crate::GitView`] presents itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GitViewOptions {
    /// Pane layout for hosts: a one-row toolbar, branches in a toggleable
    /// drawer, and the list and diff stacked when narrower than
    /// [`crate::WIDE_LAYOUT_MIN_WIDTH`] (side by side when wider).
    pub compact: bool,
    /// Multi-repository mode: the Repositories home, quick open between
    /// repositories, and the open repositories restored at launch. Off = one
    /// repository at a time (see [`crate::GitView::set_repo`]), nothing
    /// restored or saved as the FerrumGIT app's open tabs.
    pub show_repo_tabs: bool,
    /// The shared Ferrum project bar above the toolbar.
    pub show_project_bar: bool,
    /// "Open" buttons on diffs and file history that emit
    /// [`GitViewEvent::OpenFile`] for the host editor.
    pub open_files_in_host: bool,
    /// The compact layout's toolbar row (branch, fetch / pull / push and
    /// the other repository buttons). Off for hosts
    /// that offer those actions themselves (see [`crate::GitView::remote`]);
    /// the compact page tabs stay.
    pub show_toolbar: bool,
    /// The Ferrum account control (Sign in / avatar) at the right end of
    /// the top-right header; the stand-alone app only.
    pub show_account: bool,
}

impl GitViewOptions {
    /// The FerrumGIT app: full chrome, tabs restored, no host editor.
    #[must_use]
    pub const fn desktop() -> Self {
        Self {
            compact: false,
            show_repo_tabs: true,
            show_project_bar: true,
            open_files_in_host: false,
            show_toolbar: true,
            show_account: true,
        }
    }

    /// A split pane in another Ferrum app: compact, one repository, no app
    /// chrome, files open in the host.
    #[must_use]
    pub const fn embedded() -> Self {
        Self {
            compact: true,
            show_repo_tabs: false,
            show_project_bar: false,
            open_files_in_host: true,
            show_toolbar: true,
            show_account: false,
        }
    }

    /// Without the toolbar (see [`Self::show_toolbar`]).
    #[must_use]
    pub const fn without_toolbar(mut self) -> Self {
        self.show_toolbar = false;
        self
    }
}

/// A remote operation a host asks for, on the shown repository.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCommand {
    Fetch,
    Pull,
    Push,
    /// Pull, then push.
    Sync,
}

/// The shown repository's head, for a host's status line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RepoSummary {
    /// `None` when detached.
    pub branch: Option<String>,
    pub ahead: u32,
    pub behind: u32,
    /// Files with uncommitted changes.
    pub changed: usize,
    /// The running operation ("Pushing"…).
    pub busy: Option<String>,
    /// The error the view is showing (a failed push…), until dismissed.
    pub error: Option<String>,
}

impl Default for GitViewOptions {
    fn default() -> Self {
        Self::desktop()
    }
}

/// Emitted by a [`crate::GitView`]; subscribe with `cx.subscribe`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GitViewEvent {
    /// The user asked to open a working-tree file (absolute path) in the
    /// host editor, optionally at a 1-based line.
    OpenFile { path: PathBuf, line: Option<u32> },
    /// The shown repository changed (`None` = the repository list).
    RepoChanged { path: Option<PathBuf> },
    /// A commit was created (or amended) in `repo`.
    CommitCreated {
        repo: PathBuf,
        summary: String,
        amend: bool,
    },
}

/// Background services shared by every `GitView` of the process: one Tokio
/// runtime and one SQLite store, created on first use.
#[derive(Clone)]
pub struct GitServices {
    pub services: AppServices,
    /// Why the default data directory could not be used (the services then
    /// run on a temporary one); shown once by each new view.
    pub startup_warning: Option<String>,
}

impl Global for GitServices {}

/// Marks that [`init`] ran (bindings must not be registered twice).
struct Initialized;

impl Global for Initialized {}

/// Where FerrumGIT keeps its database: `FERRUMGIT_DATA_DIR`, else the
/// platform data directory.
#[must_use]
pub fn default_data_dir() -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os("FERRUMGIT_DATA_DIR") {
        return Some(PathBuf::from(dir));
    }
    directories::ProjectDirs::from("dev", "FerrumGIT", "FerrumGIT")
        .map(|dirs| dirs.data_dir().to_path_buf())
}

/// Registers FerrumGIT's key bindings (scoped to the `FerrumGit` key
/// context, so they only apply while a `GitView` has focus). Safe to call
/// more than once. Hosts call it at startup, next to their own bindings.
pub fn init(cx: &mut App) {
    if cx.has_global::<Initialized>() {
        return;
    }
    cx.set_global(Initialized);
    let context = Some(KEY_CONTEXT);
    cx.bind_keys([
        KeyBinding::new("cmd-o", QuickOpen, context),
        KeyBinding::new("cmd-p", QuickOpen, context),
        KeyBinding::new("ctrl-o", QuickOpen, context),
        KeyBinding::new("ctrl-p", QuickOpen, context),
        KeyBinding::new("cmd-r", Reload, context),
        KeyBinding::new("ctrl-r", Reload, context),
        KeyBinding::new("cmd-[", GoBack, context),
        KeyBinding::new("cmd-]", GoForward, context),
        KeyBinding::new("escape", ferrum_ui::menu::Cancel, context),
        KeyBinding::new("up", ferrum_ui::menu::SelectPrevious, Some(PICKER_CONTEXT)),
        KeyBinding::new("down", ferrum_ui::menu::SelectNext, Some(PICKER_CONTEXT)),
    ]);
}

/// [`init`], using already started services (the FerrumGIT app starts them
/// before opening its window so it can exit on failure).
pub fn init_with_services(services: AppServices, cx: &mut App) {
    init(cx);
    if !cx.has_global::<GitServices>() {
        cx.set_global(GitServices {
            services,
            startup_warning: None,
        });
    }
}

/// The process's services, started on first use. If the default data
/// directory cannot be opened, FerrumGIT runs on a temporary one and says so.
pub(crate) fn services(cx: &mut App) -> GitServices {
    init(cx);
    if let Some(services) = cx.try_global::<GitServices>() {
        return services.clone();
    }
    let started = start_services();
    cx.set_global(started.clone());
    started
}

fn start_services() -> GitServices {
    let attempt = default_data_dir()
        .ok_or_else(|| "no home directory for FerrumGIT's local state".to_owned())
        .and_then(|dir| AppServices::start(&dir).map_err(|e| format!("{}: {e}", dir.display())));
    match attempt {
        Ok(services) => GitServices {
            services,
            startup_warning: None,
        },
        Err(error) => {
            tracing::warn!(error = %error, "FerrumGIT data directory unavailable");
            let fallback = std::env::temp_dir().join(format!("ferrumgit-{}", std::process::id()));
            #[allow(clippy::expect_used)]
            let services = AppServices::start(&fallback)
                .expect("FerrumGIT could not start its background services");
            GitServices {
                services,
                startup_warning: Some(format!(
                    "FerrumGIT could not open its data ({error}); settings are temporary."
                )),
            }
        }
    }
}

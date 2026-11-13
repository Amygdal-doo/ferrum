//! What a [`crate::TermView`] tells its host (subscribe with
//! `cx.subscribe(&term_view, …)`).

use crate::connection::Backend;
use ferrumterm_protocol::{BlockId, ProgramStatus};

/// Identifies a [`crate::TermView::run_command`] call in later events.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CommandId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub enum TermViewEvent {
    /// Connected (or reconnected) to a terminal server.
    Connected(Backend),
    /// The view's title changed: the focused pane's title and the session,
    /// e.g. `vim main.rs — editor:app`.
    TitleChanged(String),
    /// A pane's program reported a status (OSC 7501, `fterm status`, Claude
    /// Code hooks): working / blocked / done / error / idle; `None` cleared it.
    AgentStatus {
        block: BlockId,
        /// The pane's label, title or program, as in the sidebar.
        title: String,
        status: Option<ProgramStatus>,
    },
    /// A [`crate::TermView::run_command`] tab started running in `block`.
    CommandStarted { command: CommandId, block: BlockId },
    /// A pane's process exited. `command` is set when it was started by
    /// [`crate::TermView::run_command`] (its tab stays open showing the
    /// output until closed).
    CommandFinished {
        block: BlockId,
        command: Option<CommandId>,
        exit_code: i32,
    },
    /// A pane rang the bell.
    Bell { block: BlockId },
}

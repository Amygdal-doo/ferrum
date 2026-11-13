//! Chat modes: Ask (read-only tools), Agent (all tools), Plan (read-only
//! tools plus `write_plan`; nothing is edited until the plan is built).

use serde::{Deserialize, Serialize};

use crate::tools::Access;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Mode {
    Ask,
    #[default]
    Agent,
    Plan,
}

impl Mode {
    pub const ALL: [Mode; 3] = [Mode::Agent, Mode::Ask, Mode::Plan];

    pub fn label(self) -> &'static str {
        match self {
            Mode::Ask => "Ask",
            Mode::Agent => "Agent",
            Mode::Plan => "Plan",
        }
    }

    /// Shift-Tab order.
    pub fn next(self) -> Mode {
        match self {
            Mode::Agent => Mode::Ask,
            Mode::Ask => Mode::Plan,
            Mode::Plan => Mode::Agent,
        }
    }

    pub fn allows(self, access: Access) -> bool {
        match (self, access) {
            (_, Access::Read) => true,
            (Mode::Agent, Access::Plan) => false,
            (Mode::Agent, _) => true,
            (Mode::Plan, Access::Plan) => true,
            _ => false,
        }
    }

    /// Mode-specific instructions for the system prompt.
    pub fn instructions(self) -> &'static str {
        match self {
            Mode::Ask => {
                "You are in Ask mode: answer questions about the code. You can read and search \
                 files but cannot edit files or run commands. Suggest changes as code blocks."
            }
            Mode::Agent => {
                "You are in Agent mode: make the requested changes with your tools. Read before \
                 you edit, prefer edit_file with a unique old_string for small changes, and keep \
                 edits minimal. Every edit is reviewed by the user, who can accept or reject it."
            }
            Mode::Plan => {
                "You are in Plan mode: research the code with the read-only tools, ask \
                 clarifying questions when the request is ambiguous, then call write_plan with a \
                 concise markdown plan (goal, files to change, steps, risks). Do not edit files; \
                 the user will review the plan and press Build to carry it out."
            }
        }
    }
}

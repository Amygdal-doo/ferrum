//! FerrumChat core: everything behind the agentic coding chat that does not
//! draw pixels.
//!
//! - [`provider`]: streaming model providers (Anthropic Messages, OpenAI
//!   Chat Completions / Responses, Ollama) behind one [`provider::Provider`]
//!   trait, plus a scripted [`provider::mock::MockProvider`] for tests.
//! - [`agent`]: the loop that streams a model, runs its tool calls through
//!   [`permissions`] and the sandboxed [`tools`], and feeds results back.
//! - [`changeset`]: every file the agent writes goes through a change set
//!   against a [`snapshot`], reviewable per hunk ([`review`]) with per-turn
//!   checkpoints that can be restored (and the restore undone).
//! - [`rules`], [`context`]: rules files and @-mentions for the prompt.
//! - [`mcp`], [`acp`]: Model Context Protocol tools and Agent Client
//!   Protocol agents (Claude Code, Codex, Gemini CLI) in the same thread
//!   model.
//! - [`thread`], [`store`]: the thread model and its JSON persistence;
//!   [`session`] ties it all together for a UI.
//! - [`status`]: each thread's Needs you / Done / Working / Idle state, a
//!   pure state machine a UI feeds with session events.
//!
//! Async code runs on tokio ([`runtime::handle`]); a UI receives
//! [`session::SessionEvent`]s on a `futures` channel any executor can poll.

pub mod acp;
pub mod agent;
pub mod changeset;
pub mod context;
pub mod custom_models;
pub mod fsutil;
pub mod jsonrpc;
pub mod mcp;
pub mod models;
pub mod modes;
pub mod paths;
pub mod permissions;
pub mod provider;
pub mod review;
pub mod rules;
pub mod runtime;
pub mod secrets;
pub mod session;
pub mod settings;
pub mod snapshot;
pub mod status;
pub mod store;
pub mod thread;
pub mod tools;

pub use changeset::{ChangeSet, CheckpointId};
pub use modes::Mode;
pub use session::{ChatSession, SessionConfig, SessionEvent};
pub use thread::{Thread, ThreadEvent};

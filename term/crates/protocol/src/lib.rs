//! FerrumTerm client/server protocol.
//!
//! The server owns every terminal. Clients (the GPU app, the `fterm` CLI,
//! `fterm attach`, agents) talk to it over a Unix socket — or token-authenticated
//! TCP on a tailnet — with length-prefixed JSON messages: `u32` big-endian
//! length, then a [`ClientMsg`] or [`ServerMsg`].

pub mod forward;
mod frame;
mod ids;
mod msg;
pub mod status;
mod wire;

pub use forward::{ForwardInfo, ForwardKind, ForwardSpec, ForwardState, forward_key};
pub use frame::{Cursor, CursorShape, Frame, Span, flags};
pub use ids::{BlockId, SessionId, WindowId};
pub use msg::*;
pub use status::{OscScanner, ProgramState, ProgramStatus, StatusUpdate};
pub use wire::{MAX_FRAME, WireError, read_msg, write_msg};

/// Protocol version; bumped on incompatible changes.
pub const PROTOCOL_VERSION: u32 = 2;
/// Default TCP port for remote (tailnet) listeners.
pub const DEFAULT_PORT: u16 = 7397;

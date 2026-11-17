//! FerrumTerm client/server protocol.
//!
//! The server owns every terminal. Clients (the GPU app, the `fterm` CLI,
//! `fterm attach`, agents) talk to it over a Unix socket — or token-authenticated
//! TCP on a tailnet — with length-prefixed JSON messages: `u32` big-endian
//! length, then a [`ClientMsg`] or [`ServerMsg`].

pub mod agents;
pub mod forward;
mod frame;
mod ids;
mod msg;
pub mod status;
pub mod version;
mod wire;

pub use forward::{ForwardInfo, ForwardKind, ForwardSpec, ForwardState, forward_key};
pub use frame::{CommandMark, Cursor, CursorShape, Frame, Span, flags};
pub use ids::{BlockId, SessionId, WindowId};
pub use msg::*;
pub use status::{OscScanner, ProgramState, ProgramStatus, StatusUpdate};
pub use version::{SERVER_REVISION, ServerAge, build_version};
pub use wire::{MAX_FRAME, WireError, read_msg, write_msg};

/// Protocol version; bumped on incompatible changes.
pub const PROTOCOL_VERSION: u32 = 2;
/// `Hello { client }` of the FerrumTerm window. While one is connected it
/// delivers agent notifications itself (knowing which pane is on screen), so
/// the server only posts them when no window is open.
pub const WINDOW_CLIENT: &str = "ferrumterm-window";
/// Default TCP port for remote (tailnet) listeners.
pub const DEFAULT_PORT: u16 = 7397;

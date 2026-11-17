//! FerrumPorts core: which processes listen on which ports, and what to do
//! about them. No GPUI; the parsers are pure and tested against captured
//! `lsof`, `ps` and `ss` output. [`remote`] runs the same tools on another
//! machine over `ssh`.

pub mod forward;
pub mod free;
pub mod hints;
pub mod kill;
pub mod lsof;
pub mod model;
pub mod project;
pub mod ps;
pub mod remote;
pub mod scan;
pub mod ss;
pub mod table;

pub use free::{PortCheck, check_port, find_free_port_near, is_port_free};
pub use hints::{Hint, hint};
pub use kill::{KillError, Signal};
pub use model::{Exposure, Listener, ListenerKey, Protocol, Snapshot};
pub use scan::{ScanError, ScanOptions, scan};
pub use table::{Row, View};

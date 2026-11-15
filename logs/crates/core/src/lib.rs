//! Logs core (behind FerrumIDE's Logs page): everything below the UI.
//!
//! - [`source`] / [`store`]: a project's saved sources (files, commands,
//!   Docker containers, compose services) in `~/.ferrum/logs/projects.json`;
//! - [`suggest`]: sources a project probably has;
//! - [`runner`] / [`tail`]: running them on background threads;
//! - [`line`] / [`level`] / [`ansi`]: parsing each line (colours, level,
//!   JSON message and fields, timestamps);
//! - [`buffer`] / [`filter`] / [`session`]: the bounded history, filters
//!   and the per-project state the desktop app renders.

pub mod ansi;
pub mod buffer;
pub mod filter;
pub mod glob;
pub mod level;
pub mod line;
pub mod runner;
pub mod session;
pub mod source;
pub mod store;
pub mod suggest;
pub mod tail;

pub use buffer::LogBuffer;
pub use filter::{Filter, Levels, Matcher, View};
pub use level::Level;
pub use line::{LogLine, Stream};
pub use runner::{Event, RunStatus};
pub use session::{Health, Launcher, Session, SourceState};
pub use source::{SourceConfig, SourceKind};
pub use store::Store;

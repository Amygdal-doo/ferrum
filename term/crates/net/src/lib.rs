//! Networking helpers for FerrumTerm that need no GUI or server:
//!
//! - [`ssh_config`]: read-only `~/.ssh/config` host aliases;
//! - [`hosts`]: hosts saved in FerrumTerm and their port-forward definitions;
//! - [`args`]: command lines for the system `ssh` (tabs and `ssh -N` forwards);
//! - [`ports`]: which process listens on a TCP port, and signalling it.

pub mod args;
pub mod hosts;
pub mod ports;
pub mod ssh_config;

pub use hosts::{Host, HostSource, HostStore, SavedHost};

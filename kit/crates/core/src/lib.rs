//! FerrumKit core: every tool of the FerrumKit desktop app as pure functions.
//!
//! Nothing here touches the network or the UI, and only [`store`] (saved
//! inputs and history) touches the file system; inputs are strings,
//! outputs are strings or small structs. The GPUI shell in
//! `kit/apps/desktop` only wires these to text fields.

// Tests unwrap freely; production code must not.
#![cfg_attr(test, allow(clippy::unwrap_used))]

pub mod base64;
pub mod case;
pub mod catalog;
pub mod color;
pub mod cron;
pub mod data_format;
pub mod diff;
pub mod hash;
pub mod ids;
pub mod jwt;
pub mod lines;
pub mod number_base;
pub mod regex_tool;
pub mod store;
pub mod timestamp;
pub mod url_tool;

pub use catalog::{Tool, filter_tools};

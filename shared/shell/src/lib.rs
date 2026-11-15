//! Shell commands for the Ferrum apps and the preferences file they share.
//!
//! - `fei .`, `feg`, `fet`, … — one short command per app (see
//!   [`apps::APPS`]), each a small script in `~/.ferrum/bin`;
//! - `ferrum <app> [path]` — one command for all of them ([`umbrella`]);
//! - `~/.ferrum/preferences.json` — which commands are on and their names,
//!   and which apps run without a Dock icon ([`prefs`]). FerrumLauncher and
//!   every app's Settings › System read and write it, so both always agree.
//!
//! Pure functions plus small file operations; no GUI, no async, so the
//! launcher core and `ferrum-ui` can both use it.

pub mod apps;
pub mod names;
pub mod ops;
pub mod path;
pub mod prefs;
pub mod script;
pub mod umbrella;

pub use apps::{APPS, App};
pub use names::{check, validate};
pub use ops::Env;
pub use path::{Shell, ensure_on_path};
pub use prefs::{COMMANDS_REVISION, CommandPref, SharedPrefs};
pub use script::MARKER;
